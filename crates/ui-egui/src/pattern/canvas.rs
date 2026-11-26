//! The FX-Map editor's canvas: the framework node graph on an unbounded canvas, and the mapping of its events onto
//! pattern edits.

use super::PatternEditor;
use super::kind::FxMapKind;
use super::pattern_editor::{PatternWire, wires};
use crate::MaterialCraftApp;
use crate::graph::{LibraryDrag, PortHit, wire_id};
// PLAN: `graph/mod.rs` declares `annotations` private (`mod annotations;`); graph-w10 makes it `pub(crate)` for the
// Function and FX-Map hosts.
use crate::graph::annotations;
use craft_behavior::canvas::CanvasView;
use craft_behavior::kurbo::{Point, Rect as DocRect, Vec2 as DocVec};
use craft_behavior::value::EditEvent;
use craft_behavior::{Selection, SelectionModel as _, keyed};
use craft_ui::canvas::{Canvas, NavigationOptions, ViewCommands, ViewRequest, ViewTarget, WheelScheme, canvas_space};
use craft_ui::canvas_controls::CanvasGrid;
use craft_ui::contract::{Bind, CollectionEvent, DropAt, DropKind, DropPayload, DropPlace, Props as _, Show as _, ShowCollection as _, ShowContainer as _};
use craft_ui::graph::{AnnotationEvent, Node, NodeEvent, NodeGraph, NodeId, NodeKind, PortLabels, PortRef, PortSide, Wire, wire_controls};
use craft_ui::runtime::env::with_env;
use egui::{Pos2, Rect, Ui, Vec2, pos2, vec2};
use materialcraft_engine::PatternEdit;
use materialcraft_engine::core::{NodeId as GraphNodeId, PatternGraph};
use materialcraft_engine::nodes::PortKind;
use serde_json::json;

/// How far above a frame a fit reaches, so the frame's title shows.
const FRAME_TITLE: f32 = 32.0;

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

/// Shows the open FX-Map. Every event comes from the input its own pass applied: the framework reports a press, a
/// drag start and a release once, in the pass that took that input, the given node selection is rebuilt from
/// `editor.selection` each pass, and the queued view commands are taken, so a second pass in a frame applies nothing
/// again.
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Some(mut editor) = app.pattern_editor.take() else { return };

    if let Some(graph) = editor.current(app) {
        let present: std::collections::BTreeSet<_> = graph.nodes.iter().map(|node| node.id).collect();
        editor.selection.retain(|id| present.contains(id));
    }
    canvas(app, &mut editor, ui);
    editor.inspected_node = crate::graph::node_selection::inspected(&editor.selection, editor.inspected_node);
    app.pattern_editor = Some(editor);
}

fn canvas(app: &mut MaterialCraftApp, editor: &mut PatternEditor, ui: &mut Ui) {
    // A copy, so the kinds borrow the graph while the pass's events edit the session.
    let Some(graph) = editor.current(app).cloned() else { return };
    let snap = app.ui.prefs.snap_to_grid;
    let pixels_per_point = ui.ctx().pixels_per_point();

    // `NodeId(i)` is `ids[i]`: one vector names the nodes for the rects, the kinds, the selection and the wires.
    let ids: Vec<u32> = graph.nodes.iter().map(|node| node.id).collect();
    let index = |id: u32| ids.iter().position(|node| *node == id).map(|at| NodeId::new(at as u32));
    let keys: Vec<NodeId> = (0..ids.len()).map(|at| NodeId::new(at as u32)).collect();

    // A wire whose child slot was emptied leaves the selection.
    let live: Vec<PatternWire> = wires(&graph);
    let live_ids: Vec<u64> = live.iter().map(|wire| wire_id(&wire.wire)).collect();
    editor.selected_wires.items.retain(|id| live_ids.contains(id));
    let gone = |key: &Option<u64>| key.is_some_and(|key| !live_ids.contains(&key));
    if gone(&editor.selected_wires.lead) || gone(&editor.selected_wires.anchor) {
        let items = std::mem::take(&mut editor.selected_wires.items);
        editor.selected_wires = Selection::new(items.clone(), items.last().copied(), items.first().copied());
    }

    if app.ui.frame_all {
        editor.queued.push(ViewRequest::Command(ViewCommands::FitOnScreen));
    }
    app.ui.frame_all = false;

    let layers = annotations::layers(app);
    let (backdrops, notes, markers) = (layers.backdrops(), layers.notes(), layers.markers());
    let carried: Option<(Vec<u32>, Vec2)> = layers.carried().map(|(contents, by)| (contents.function_nodes.clone(), by));

    // The move preview shifts the selected nodes as the commit will; a press that has not moved shifts nothing.
    let moving = app.graph.node_move.filter(|by| by.length() > 0.5);
    let kinds: Vec<FxMapKind> = graph.nodes.iter().map(|node| FxMapKind::new(node, graph.root == Some(node.id))).collect();
    let rects: Vec<Rect> = graph
        .nodes
        .iter()
        .zip(&kinds)
        .map(|(node, kind)| {
            let mut pos = node.position;
            if let Some(by) = moving
                && editor.selection.contains(&node.id)
            {
                pos = materialcraft_engine::graph_grid::translate(pos, by.into(), snap);
            }
            let rect = kind.rect(pos);
            match &carried {
                Some((nodes, by)) if nodes.contains(&node.id) => rect.translate(*by),
                _ => rect,
            }
        })
        .collect();
    let graph_wires: Vec<Wire> = live
        .iter()
        .zip(&live_ids)
        .filter_map(|(wire, id)| Some(Wire::new(*id, PortRef::new(index(wire.parent)?, 1 + wire.slot), PortRef::new(index(wire.child)?, 0))))
        .collect();

    // Fit on Screen frames every node and every frame with its title; Fit Layers the selection, else every node.
    let frames = graph
        .annotations
        .frames
        .iter()
        .map(|frame| Rect::from_min_size(pos2(frame.rect[0], frame.rect[1] - FRAME_TITLE), vec2(frame.rect[2], frame.rect[3] + FRAME_TITLE)));
    let all = rects.iter().copied().chain(frames).fold(Rect::NOTHING, Rect::union);
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
    navigation.wheel = WheelScheme::Zoom;
    navigation.pinch_zooms = true;
    navigation.invert_wheel_zoom = app.ui.prefs.invert_zoom;
    let source = keyed(&keys, |key| *key);
    let annotation = layers.selected;
    let start_view = editor.view;
    let kind_refs: Vec<&dyn NodeKind<()>> = kinds.iter().map(|kind| kind as &dyn NodeKind<()>).collect();
    let wire_selection = &mut editor.selected_wires;
    // PLAN: graph-w6 re-exports ge.A1's `graph_zoom` from `state/mod.rs` (`mod graph_camera` is private there).
    let shown = with_env(
        ui,
        |scope| {
            scope.values.set(crate::state::graph_zoom(pixels_per_point));
        },
        |ui| {
            Canvas::new(tr("FX-Map graph"))
                .document(ViewTarget::unbounded(extent).fit_ceiling(1.5 * f64::from(pixels_per_point)))
                .layer_bounds(&layer_bounds)
                .queued(&queued)
                .navigation(navigation)
                .view(start_view)
                .show(ui, |ui, body| {
                    let space = canvas_space(&body.env)?;
                    let mut nodes = NodeGraph::new(&rects, &kind_refs, &graph_wires)
                        .label(tr("FX-Map nodes"))
                        .command_scope(crate::commands::GRAPH_SCOPE)
                        .selection(Bind::given(given.clone()))
                        .wire_selection(Bind::bound(&mut *wire_selection))
                        .port_labels(PortLabels::OnHover)
                        .backdrops(&backdrops)
                        .notes(&notes)
                        .markers(&markers)
                        .annotation_selection(Bind::given(annotation))
                        .takes_drop(DropKind::new::<LibraryDrag>(DropPlace::Spot, |_, _| true));
                    if let Some(zoom) = space.view.layer_transform() {
                        nodes = nodes.zoom(zoom);
                    }
                    let response = nodes.show_items(ui, &source, &mut |key, _| Node::new(*key, kind_refs[key.0 as usize]));
                    // In the picture slot, under the nodes.
                    let step = f64::from(materialcraft_engine::graph_grid::GRID_SPACING);
                    let _ = CanvasGrid::new(tr("Grid")).spacing(DocVec::new(step, step)).picture().show_at(ui, body.rect);
                    Some((response, space.view))
                })
        },
    );
    editor.view = shown.output.view.unwrap_or(editor.view);
    let Some(Some((response, view))) = shown.inner else { return };

    let mut selection = given;
    for event in &response.collection.events {
        match event {
            CollectionEvent::Selection { change, .. } => {
                selection.apply(change.clone());
                editor.selection = selection.items.iter().filter_map(|node| ids.get(node.0 as usize)).copied().collect();
            }
            CollectionEvent::Move(edit) => moved(app, editor, edit, snap),
            CollectionEvent::Item(key, event) => match event {
                // The output end is the parent's slot and the input end the child, whichever end the drag started on.
                NodeEvent::Wired { from, to, .. } => {
                    let (Some(&parent), Some(&child)) = (ids.get(from.node.0 as usize), ids.get(to.node.0 as usize)) else { continue };
                    let Some(slot) = kinds.get(from.node.0 as usize).and_then(|kind| kind.port_slot(from.port)) else { continue };
                    editor.edit(app, PatternEdit::Connect { parent, slot, child }, false);
                }
                // Alt-click on a port empties it: a slot loses its child, the input leaves every parent.
                NodeEvent::Clicked { port: Some(port), mods, .. } if mods.alt => {
                    let (Some(&node), Some(kind)) = (ids.get(key.0 as usize), kinds.get(key.0 as usize)) else { continue };
                    editor.edit(app, PatternEdit::Disconnect { node, slot: kind.port_slot(*port) }, false);
                }
                _ => {}
            },
            CollectionEvent::Dropped { at: DropAt::Spot { at, .. }, payload, .. } => dropped(app, editor, payload, *at, snap),
            CollectionEvent::EmptyContextMenu { at, .. } => editor.open_menu(to_graph(&view, *at)),
            CollectionEvent::ContextMenu(request) => editor.open_menu(to_graph(&view, request.at)),
            _ => {}
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
            AnnotationEvent::ContextMenu { at, .. } => editor.open_menu(to_graph(&view, *at)),
            _ => {}
        }
    }

    publish(app, &graph, &kinds, &ids, &rects, &live, &view, &layers);
    // A menu Space opened has no request this pass: it opens at its graph point.
    let pointer = Some(to_screen(&view, editor.menu_at));
    super::context_menu::show(app, editor, ui, &response, pointer);
}

/// A node move: the update previews the offset, the commit moves the selection by it; a press with no drag moves
/// nothing, as the offset stays under half a unit.
fn moved(app: &mut MaterialCraftApp, editor: &mut PatternEditor, edit: &EditEvent<Vec2>, snap: bool) {
    match edit {
        EditEvent::Update { value, .. } => app.graph.node_move = Some(*value),
        EditEvent::Commit { after: by, .. } | EditEvent::Set { after: by, .. } => {
            app.graph.node_move = None;
            if by.length() > 0.5 && !editor.selection.is_empty() {
                editor.edit(app, PatternEdit::Move { ids: editor.selection.clone(), by: (*by).into(), snap }, false);
            }
        }
        EditEvent::Cancel { .. } => app.graph.node_move = None,
        _ => {}
    }
}

/// A library entry dropped at graph point `at`: the new node is selected, unless the entry is an annotation, which
/// holds the selection alone.
fn dropped(app: &mut MaterialCraftApp, editor: &mut PatternEditor, payload: &DropPayload, at: Pos2, snap: bool) {
    let Some(entry) = payload.get::<LibraryDrag>() else { return };
    let Some(result) = app.run("library.add", json!({"entry": entry.0, "at": [at.x, at.y], "snapToGrid": snap})) else { return };
    let new_id = result.get("result").and_then(|result| result.get("id")).and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok());
    editor.selection = if app.session.annotation_selection.is_some() { Vec::new() } else { new_id.into_iter().collect() };
    editor.selected_wires = Selection::default();
}

/// Stores what the pass drew, in screen points, for `ui.inspect` and scripted input.
fn publish(
    app: &mut MaterialCraftApp,
    graph: &PatternGraph,
    kinds: &[FxMapKind],
    ids: &[u32],
    rects: &[Rect],
    live: &[PatternWire],
    view: &CanvasView,
    layers: &annotations::AnnotationLayers,
) {
    let port_at = |at: usize, port: usize| rects.get(at).zip(kinds.get(at)).map(|(rect, kind)| rect.min + kind.port_offset(port, rect.size()));
    let screen_rects =
        ids.iter().zip(rects).map(|(id, rect)| (graph_id(*id), Rect::from_min_max(to_screen(view, rect.min), to_screen(view, rect.max)))).collect();
    let mut ports = Vec::new();
    for (at, (id, kind)) in ids.iter().zip(kinds).enumerate() {
        for port in 0..kind.port_count() {
            let Some(pos) = port_at(at, port) else { continue };
            ports.push(PortHit {
                node: graph_id(*id),
                output: kind.port_slot(port).is_some(),
                port: kind.port_id(port),
                label: kind.port_name(port),
                pos: to_screen(view, pos),
                kind: PortKind::Any,
            });
        }
    }
    let at_of = |id: u32| graph.nodes.iter().position(|node| node.id == id);
    let wires = live
        .iter()
        .filter_map(|wire| {
            let start = port_at(at_of(wire.parent)?, 1 + wire.slot)?;
            let end = port_at(at_of(wire.child)?, 0)?;
            Some((wire.wire.clone(), wire_controls(start, PortSide::Bottom, end, PortSide::Top).map(|point| to_screen(view, point))))
        })
        .collect();
    let (frames, comments, pins) = annotations::screen_rects(layers, view);
    app.graph.publish(view.rect, screen_rects, ports, wires, annotations::screen_rects(layers, view));
}
