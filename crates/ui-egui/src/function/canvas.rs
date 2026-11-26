//! The Function editor's canvas: the framework node graph on an unbounded canvas, and the mapping of its events onto
//! Function edits.

use super::function_editor::{FunctionWire, wires};
use super::library::FunctionLibraryDrag;
use super::node_kind::{FunctionKind, FunctionValueEdit};
use super::{FunctionEditor, FunctionTarget};
use crate::MaterialCraftApp;
use crate::graph::editor_target::EditorTarget;
use crate::graph::{NODE_W, PortHit, WireRef, wire_id};
use crate::graph::annotations;
use crate::graph::node_search::SearchWire;
use crate::graph::search::{self, SearchEditor};
use craft_behavior::canvas::CanvasView;
use craft_behavior::kurbo::{Point, Rect as DocRect, Vec2 as DocVec};
use craft_behavior::value::EditEvent;
use craft_behavior::{Mixed, Selection, SelectionModel as _, keyed};
use craft_ui::canvas::{Canvas, NavigationOptions, ViewTarget, WheelScheme, canvas_space};
use craft_ui::canvas_controls::CanvasGrid;
use craft_ui::contract::{Bind, CollectionEvent, DropAt, DropKind, DropPayload, DropPlace, Props as _, Show as _, ShowCollection as _, ShowContainer as _};
use craft_ui::graph::{AnnotationEvent, Node, NodeEvent, NodeGraph, NodeId, NodeKind, PortLabels, PortRef, PortSide, Wire, WireEvent, wire_controls};
use craft_ui::runtime::env::with_env;
use egui::{Pos2, Rect, Ui, Vec2, pos2, vec2};
use materialcraft_engine::FunctionEdit;
use materialcraft_engine::core::NodeId as GraphNodeId;
use serde_json::json;

/// How far above a frame a fit reaches, so the frame's title shows.
const FRAME_TITLE: f32 = 32.0;

/// The engine id of Function node `id`, which the published rects and ports carry.
fn graph_id(id: u32) -> GraphNodeId {
    GraphNodeId(u64::from(id))
}

fn doc_rect(rect: Rect) -> DocRect {
    DocRect::new(f64::from(rect.min.x), f64::from(rect.min.y), f64::from(rect.max.x), f64::from(rect.max.y))
}

/// Screen point `at` in graph units.
fn to_graph(view: &CanvasView, at: Pos2) -> Pos2 {
    let at = view.to_doc(at);
    pos2(at.x as f32, at.y as f32)
}

/// Graph point `at` on screen.
fn to_screen(view: &CanvasView, at: Pos2) -> Pos2 {
    view.to_screen(Point::new(f64::from(at.x), f64::from(at.y)))
}

/// The editor's selection as the graph's: `NodeId(i)` is `ids[i]`, the last pick is the lead.
fn given_selection(selection: &[u32], ids: &[u32]) -> Selection<NodeId> {
    let items: Vec<NodeId> = selection.iter().filter_map(|id| ids.iter().position(|node| node == id)).map(|at| NodeId::new(at as u32)).collect();
    let lead = items.last().copied();
    Selection::new(items, lead, lead)
}

/// The graph's selection as the editor's Function ids.
fn picked(selection: &Selection<NodeId>, ids: &[u32]) -> Vec<u32> {
    selection.items.iter().filter_map(|node| ids.get(node.0 as usize)).copied().collect()
}

/// The canvas of the open Function graph. Every event comes from the input its own pass applied: the framework reports
/// a press, a drag start and a release once, in the pass that took that input, and the given selection is rebuilt
/// from `editor.selection` each pass, so a second pass in a frame applies nothing again.
pub(super) fn canvas(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, ui: &mut Ui) {
    // A copy, so the kinds borrow the graph while the pass's events edit the session.
    let Some(graph) = editor.current(app).cloned() else { return };
    let snap = app.ui.prefs.snap_to_grid;
    let pixels_per_point = ui.ctx().pixels_per_point();

    // `NodeId(i)` is `ids[i]`: one vector names the nodes for the rects, the kinds, the selection and the menu.
    let ids: Vec<u32> = graph.nodes.iter().map(|node| node.id).collect();
    let index = |id: u32| ids.iter().position(|node| *node == id).map(|at| NodeId::new(at as u32));
    let keys: Vec<NodeId> = (0..ids.len()).map(|at| NodeId::new(at as u32)).collect();

    // A node that left the graph leaves the selection, and so does a wire whose input is no longer fed.
    editor.selection.retain(|id| ids.contains(id));
    let live: Vec<FunctionWire> = wires(&graph).collect();
    let live_ids: Vec<u64> = live.iter().map(|wire| wire_id(&wire.wire)).collect();
    editor.selected_wires.items.retain(|id| live_ids.contains(id));
    let gone = |key: &Option<u64>| key.is_some_and(|key| !live_ids.contains(&key));
    if gone(&editor.selected_wires.lead) || gone(&editor.selected_wires.anchor) {
        let items = std::mem::take(&mut editor.selected_wires.items);
        editor.selected_wires = Selection::new(items.clone(), items.last().copied(), items.first().copied());
    }

    if app.ui.frame_all {
        editor.queued.push(craft_ui::canvas::ViewRequest::Place(craft_ui::canvas::ViewCommands::FitOnScreen));
    }
    app.ui.frame_all = false;

    // graph-w10's accessors hand the backdrops, notes and markers as owned lists (ge.A9).
    let layers = annotations::layers(app);
    let (backdrops, notes, markers) = (layers.backdrops(), layers.notes(), layers.markers());
    let carried: Option<(Vec<u32>, Vec2)> = layers.carried().map(|(contents, by)| (contents.function_nodes.clone(), by));

    // The move preview shifts the selected nodes as the commit will; a press that has not moved shifts nothing.
    let moving = app.graph.node_move.filter(|by| by.length() > 0.5);
    let rects: Vec<Rect> = graph
        .nodes
        .iter()
        .map(|node| {
            let mut pos = node.pos;
            if let Some(by) = moving
                && editor.selection.contains(&node.id)
            {
                pos = materialcraft_engine::graph_grid::translate(pos, by.into(), snap);
            }
            let rect = FunctionKind::rect(node, pos);
            match &carried {
                Some((nodes, by)) if nodes.contains(&node.id) => rect.translate(*by),
                _ => rect,
            }
        })
        .collect();
    let graph_wires: Vec<Wire> = live
        .iter()
        .zip(&live_ids)
        .filter_map(|(wire, id)| {
            let source = graph.node(wire.from)?;
            Some(Wire::new(*id, PortRef::new(index(wire.from)?, FunctionKind::output_port_of(source)), PortRef::new(index(wire.to)?, wire.input)))
        })
        .collect();

    // Fit on Screen frames every node and every frame with its title; Fit Layers the selection, else every node.
    let frames: Vec<Rect> = app
        .graph
        .annotation_target
        .items(&app.session)
        .map(|items| {
            items
                .frames
                .iter()
                .map(|frame| Rect::from_min_size(pos2(frame.rect[0], frame.rect[1] - FRAME_TITLE), vec2(frame.rect[2], frame.rect[3] + FRAME_TITLE)))
                .collect()
        })
        .unwrap_or_default();
    let all = rects.iter().chain(&frames).fold(Rect::NOTHING, |all, rect| all.union(*rect));
    let extent = doc_rect(if all.is_positive() { all } else { Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)) });
    let chosen = rects
        .iter()
        .zip(&ids)
        .filter(|(_, id)| editor.selection.is_empty() || editor.selection.contains(id))
        .fold(Rect::NOTHING, |all, (rect, _)| all.union(*rect));
    let chosen = chosen.is_positive().then(|| doc_rect(chosen));
    let layer_bounds = || chosen;

    let given = given_selection(&editor.selection, &ids);
    let queued = std::mem::take(&mut editor.queued);
    let mut navigation = NavigationOptions::default();
    navigation.invert_wheel_zoom = app.ui.prefs.invert_zoom;
    navigation.wheel = WheelScheme::Zoom;
    navigation.pinch_zooms = true;
    let source = keyed(&keys, |key| *key);
    let annotation = layers.selected;
    let start_view = editor.view;
    let wire_selection = &mut editor.selected_wires;
    // PLAN: graph-w6 re-exports ge.A1's `graph_zoom` from `state/mod.rs` (`mod graph_camera` is private there).
    let shown = with_env(
        ui,
        |scope| {
            scope.values.set(crate::state::graph_zoom(pixels_per_point));
        },
        |ui| {
            Canvas::new(crate::i18n::tr("Function Graph"))
                .document(ViewTarget::unbounded(extent).fit_ceiling(1.5 * f64::from(pixels_per_point)))
                .layer_bounds(&layer_bounds)
                .queued(&queued)
                .navigation(navigation)
                .view(start_view)
                .show(ui, |ui, body| {
                    let space = canvas_space(&body.env)?;
                    let zoom = space.view.layer_transform();
                    let scale = zoom.map_or(1.0, |zoom| zoom.scale);
                    let kinds: Vec<FunctionKind<'_>> = graph.nodes.iter().map(|node| FunctionKind::new(node, graph.output == Some(node.id), scale)).collect();
                    let kinds: Vec<&dyn NodeKind<FunctionValueEdit>> = kinds.iter().map(|kind| kind as &dyn NodeKind<FunctionValueEdit>).collect();
                    let mut nodes = NodeGraph::new(&rects, &kinds, &graph_wires)
                        .label(crate::i18n::tr("Function nodes"))
                        .command_scope(crate::commands::GRAPH_SCOPE)
                        .selection(Bind::given(given.clone()))
                        .wire_selection(Bind::bound(&mut *wire_selection))
                        .port_labels(PortLabels::OnHover)
                        .backdrops(&backdrops)
                        .notes(&notes)
                        .markers(&markers)
                        .annotation_selection(Bind::given(annotation))
                        .takes_drop(DropKind::new::<FunctionLibraryDrag>(DropPlace::Spot, |_, _| true))
                        .takes_drop(DropKind::new::<crate::graph_drag_payload::GraphDragPayload>(DropPlace::Spot, |_, _| true))
                        .takes_drop(DropKind::new::<crate::graph::LibraryDrag>(DropPlace::Spot, |_, _| true));
                    if let Some(zoom) = zoom {
                        nodes = nodes.zoom(zoom);
                    }
                    let response = nodes.show_items(ui, &source, &mut |key, _| Node::new(*key, kinds[key.0 as usize]));
                    // In the picture slot, under the nodes.
                    let step = f64::from(materialcraft_engine::graph_grid::GRID_SPACING);
                    let _ = CanvasGrid::new(crate::i18n::tr("Grid")).spacing(DocVec::new(step, step)).picture().show_at(ui, body.rect);
                    Some((response, space.view))
                })
        },
    );
    editor.view = shown.output.view.unwrap_or(editor.view);
    let Some(Some((response, view))) = shown.inner else {
        finish(app, editor);
        return;
    };

    let wire_of = |id: u64| live.iter().zip(&live_ids).find(|(_, live)| **live == id).map(|(wire, _)| wire.wire.clone());
    let mut selection = given;
    for event in &response.collection.events {
        match event {
            CollectionEvent::Selection { change, .. } => {
                selection.apply(change.clone());
                editor.selection = picked(&selection, &ids);
            }
            CollectionEvent::Move(edit) => moved(app, editor, edit, snap),
            CollectionEvent::Item(key, event) => match event {
                NodeEvent::Wired { from, to, detached, .. } => {
                    let (Some(&from), Some(&to_id)) = (ids.get(from.node.0 as usize), ids.get(to.node.0 as usize)) else { continue };
                    // The node's drop target already refuses a wire from a node onto itself.
                    if let Some(wire) = detached.and_then(wire_of) {
                        disconnect(app, editor, wire);
                    }
                    editor.edit(app, FunctionEdit::Connect { from, to: to_id, input: to.port }, false);
                }
                NodeEvent::WireEnded { op: None, from, detached, loose, .. } => {
                    if let Some(wire) = detached.and_then(wire_of) {
                        disconnect(app, editor, wire);
                    }
                    // A refused port ends the wire without opening search, matching production's loose-release path.
                    if let Some(at) = loose
                        && app.ui.prefs.wire_drop_opens_node_search
                        && !response.collection.response.rect.contains(*at)
                        && let (Some(&id), Some(node)) = (ids.get(from.node.0 as usize), graph.nodes.get(from.node.0 as usize))
                    {
                        let wire = if from.port == FunctionKind::output_port_of(node) {
                            SearchWire::From((graph_id(id), "output".to_owned()))
                        } else {
                            SearchWire::To((graph_id(id), from.port.to_string()))
                        };
                        search::open(app, EditorTarget::Function, *at, Some(wire));
                    }
                }
                NodeEvent::Edit(FunctionValueEdit { id, edit }) => value_edit(app, editor, *id, edit),
                _ => {
                    let _ = key;
                }
            },
            CollectionEvent::Dropped { at: DropAt::Spot { at, .. }, payload, .. } => dropped(app, editor, payload, *at, snap),
            CollectionEvent::EmptyActivated { at, .. } => {
                editor.menu_at = to_graph(&view, *at);
                search::open(app, EditorTarget::Function, editor.menu_at, None);
            }
            CollectionEvent::EmptyContextMenu { at, .. } => editor.menu_at = to_graph(&view, *at),
            CollectionEvent::ContextMenu(request) => editor.menu_at = to_graph(&view, request.at),
            _ => {}
        }
    }

    for event in &response.wires {
        let (wire, at) = match *event {
            WireEvent::ContextMenu { wire, at } => (wire, to_graph(&view, at)),
            WireEvent::DoubleClicked { .. } => continue,
            _ => continue,
        };
        if let Some(wire) = wire_of(wire) {
            crate::graph::wire_actions::wire_menu(app, ui, &response.collection.response, wire, at);
        }
    }

    annotations::apply(app, &response.annotations);
    for event in &response.annotations {
        match event {
            // One selection at a time: an annotation picked clears the nodes and the wires.
            AnnotationEvent::Selected(Some(_)) => {
                editor.selection.clear();
                editor.selected_wires = Selection::default();
            }
            AnnotationEvent::ContextMenu { at, .. } => editor.menu_at = to_graph(&view, *at),
            _ => {}
        }
    }

    let (screen_rects, ports, wires) = publish_geometry(&graph.nodes, &ids, &rects, &live, &view);
    let annotation_rects = annotations::screen_rects(&response.annotation_rects);
    app.graph.publish(response.collection.response.rect, screen_rects, ports, wires, annotation_rects);
    search::show(app, ui, SearchEditor::Function(editor));
    super::context_menu::show(app, editor, ui, &response, &ids);
    finish(app, editor);
}

/// A node move: the update previews the offset, the commit moves the selection by it; a press with no drag moves
/// nothing, as the offset stays under half a unit.
fn moved(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, edit: &EditEvent<Vec2>, snap: bool) {
    match edit {
        EditEvent::Update { value, .. } => app.graph.node_move = Some(*value),
        EditEvent::Commit { after: by, .. } | EditEvent::Set { after: by, .. } => {
            app.graph.node_move = None;
            if by.length() > 0.5 && !editor.selection.is_empty() {
                editor.edit(app, FunctionEdit::Move { ids: editor.selection.clone(), by: (*by).into(), snap }, false);
            }
        }
        EditEvent::Cancel { .. } => app.graph.node_move = None,
        _ => {}
    }
}

/// An inline value edit: each update merges into the running step, the commit closes it, a cancel restores the value
/// from before the gesture.
fn value_edit(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, id: u32, edit: &EditEvent<materialcraft_engine::core::Value>) {
    let (value, merge) = match edit {
        EditEvent::Update { value, .. } => (value.clone(), true),
        EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => (after.clone(), false),
        EditEvent::Cancel { before: Some(Mixed::Same(before)), .. } => (before.clone(), true),
        _ => return,
    };
    editor.edit(app, FunctionEdit::Value { id, value }, merge);
}

/// Takes `wire` off its input.
fn disconnect(app: &mut MaterialCraftApp, editor: &FunctionEditor, wire: WireRef) {
    let target = editor.command_target().map(materialcraft_engine::SelectionTarget::Function);
    let mut wires = vec![wire];
    let mut selected = Selection::default();
    crate::graph::wire_actions::edit(app, &mut wires, &mut selected, target, "graph.wire.delete");
}

/// A drop from outside the canvas at graph point `at`: a library function or operation, a graph from the Explorer as
/// an instance, or a library entry. The new node is selected.
fn dropped(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, payload: &DropPayload, at: Pos2, snap: bool) {
    let corner = [at.x - NODE_W * 0.5, at.y - 20.0];
    let new_id = |result: &serde_json::Value| result.get("id").and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok());
    if let Some(entry) = payload.get::<FunctionLibraryDrag>() {
        super::library::insert(app, editor, entry, corner);
    } else if let Some(dragged) = payload.get::<crate::graph_drag_payload::GraphDragPayload>() {
        let Some(target) = editor.command_target() else { return };
        let parameters = json!({"document": dragged.document, "graph": dragged.name, "function": target, "at": corner, "snapToGrid": snap});
        if let Some(id) = app.run("graph.instance.add", parameters).as_ref().and_then(new_id) {
            editor.selection = vec![id];
            editor.selected_wires = Selection::default();
            editor.inspected_node = Some(id);
        }
    } else if let Some(entry) = payload.get::<crate::graph::LibraryDrag>()
        && let Some(result) = app.run("library.add", json!({"entry": entry.0, "at": [at.x, at.y], "snapToGrid": snap}))
    {
        // A pin or a comment from the library is selected as an annotation, which holds the selection alone.
        editor.selection = if app.session.annotation_selection.is_some() { Vec::new() } else { result.get("result").and_then(new_id).into_iter().collect() };
        editor.selected_wires = Selection::default();
    }
}

/// Stores what the pass drew, in screen points, for `ui.inspect` and scripted input.
fn publish_geometry(
    nodes: &[materialcraft_engine::core::FunctionNode],
    ids: &[u32],
    rects: &[Rect],
    live: &[FunctionWire],
    view: &CanvasView,
) -> (Vec<(GraphNodeId, Rect)>, Vec<PortHit>, Vec<(WireRef, [Pos2; 4])>) {
    let kinds: Vec<FunctionKind<'_>> = nodes.iter().map(|node| FunctionKind::new(node, false, 1.0)).collect();
    let port_at = |at: usize, port: usize| rects.get(at).zip(kinds.get(at)).map(|(rect, kind)| rect.min + kind.port_offset(port, rect.size()));
    let screen_rects =
        ids.iter().zip(rects).map(|(id, rect)| (graph_id(*id), Rect::from_min_max(to_screen(view, rect.min), to_screen(view, rect.max)))).collect();
    let mut ports = Vec::new();
    for (at, ((id, node), kind)) in ids.iter().zip(nodes).zip(&kinds).enumerate() {
        let output = FunctionKind::output_port_of(node);
        for port in 0..kind.port_count() {
            let Some(pos) = port_at(at, port) else { continue };
            ports.push(PortHit {
                node: graph_id(*id),
                output: port == output,
                port: if port == output { "output".to_owned() } else { port.to_string() },
                label: kind.port_name(port),
                pos: to_screen(view, pos),
                kind: materialcraft_engine::nodes::PortKind::Value,
            });
        }
    }
    let at_of = |id: u32| ids.iter().position(|node| *node == id);
    let wires = live
        .iter()
        .filter_map(|wire| {
            let (from, to) = (at_of(wire.from)?, at_of(wire.to)?);
            let start = port_at(from, nodes.get(from).map(FunctionKind::output_port_of)?)?;
            let end = port_at(to, wire.input)?;
            Some((wire.wire.clone(), wire_controls(start, PortSide::Right, end, PortSide::Left).map(|point| to_screen(view, point))))
        })
        .collect();
    (screen_rects, ports, wires)
}

/// The end of every pass: the standalone editor's camera and selection are the session's, and the inspected node
/// follows the selection.
fn finish(app: &mut MaterialCraftApp, editor: &mut FunctionEditor) {
    if editor.target == FunctionTarget::Graph {
        app.ui.graph_camera = crate::state::GraphCamera::from_view(&editor.view);
        app.session.selection = editor.selection.iter().map(|id| graph_id(*id)).collect();
    }
    editor.inspected_node = crate::graph::node_selection::inspected(&editor.selection, editor.inspected_node);
}
