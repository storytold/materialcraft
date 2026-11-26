use super::annotations::{self, AnnotationLayers};
use super::canvas_events;
use super::node_kind::MaterialKind;
use super::search::SearchEditor;
use super::{LibraryDrag, PortHit, WireRef, wire_id};
use crate::MaterialCraftApp;
use crate::commands::GRAPH_SCOPE;
use crate::explorer::ExplorerTarget;
use crate::graph_drag_payload::GraphDragPayload;
use crate::i18n::tr;
use crate::state::{GraphCamera, graph_zoom};
use craft_behavior::kurbo::{Point, Rect as DocRect, Vec2 as DocVec};
use craft_behavior::{CanvasView, Selection, keyed};
use craft_theme::variant::Tone;
use craft_ui::canvas::{Canvas, NavigationOptions, ViewCommands, ViewRequest, ViewTarget, WheelScheme, canvas_space};
use craft_ui::canvas_controls::CanvasGrid;
use craft_ui::contract::{Bind, DropKind, DropPlace, Props as _, Show as _, ShowCollection as _, ShowContainer as _};
use craft_ui::display::Text;
use craft_ui::graph::{AnnotationRect, Node, NodeGraph, NodeGraphResponse, NodeId as GraphNodeId, NodeKind, PortRef, Wire, wire_controls};
use craft_ui::runtime::env::{Env, with_env};
use craft_ui::style::TypeRole;
use egui::{Pos2, Rect, Ui, pos2, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::NodeId;
use materialcraft_engine::nodes::PortKind;
use std::collections::HashMap;

/// The graph panel: toolbar row and canvas
pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if ui.rect_contains_pointer(ui.available_rect_before_wrap()) && ui.input(|input| input.pointer.primary_pressed() || input.pointer.secondary_pressed()) {
        app.inspected_graph = None;
        app.inspected_folder = None;
    }

    app.reconcile_editors();
    if !crate::pattern::active(app) {
        app.pattern_editor = None;
        if app.session.graph().is_ok_and(|graph| graph.function.is_some()) {
            let target = crate::function::FunctionTarget::Graph;
            if app.function_editor.as_ref().is_none_or(|editor| editor.target != target) {
                app.function_editor = Some(crate::function::FunctionEditor::standalone(app.session.graph));
            }
        } else if app.function_editor.as_ref().is_some_and(|editor| matches!(editor.target, crate::function::FunctionTarget::Graph)) {
            app.function_editor = None;
        }
    }

    crate::annotation::activate(app, crate::annotation::target(app));
    super::toolbar::show(app, ui);
    if app.function_editor.is_some() {
        crate::function::show_graph(app, ui);
        return;
    }

    if crate::pattern::active(app) {
        crate::pattern::show(app, ui);
        return;
    }

    canvas(app, ui);
}

/// The fit cap in points per graph unit: Fit on Screen never magnifies a small graph past it.
const FIT_CAP: f64 = 1.5;

/// How far above a frame its title reaches, in graph units: the fit keeps the title in view.
const FRAME_TITLE: f32 = 32.0;

/// The grid's major step in graph units and its minor lines per step (lines every 32 and 256 units).
const GRID_STEP: f64 = 256.0;
const GRID_SUBDIVISIONS: u32 = 8;

/// One port of a node as the canvas publishes it: id, label, value kind and whether it is an output.
type PortInfo = (String, String, PortKind, bool);

/// The pass's nodes, in `graph.nodes` order: the framework graph's `NodeId(i)` is entry `i` of every list here, and
/// none is sorted or filtered apart from the others.
#[derive(Default)]
struct NodeList {
    ids: Vec<NodeId>,
    /// Each node's card in graph units, moved by the running move preview.
    positions: Vec<Rect>,
    kinds: Vec<MaterialKind>,
    /// Each node's ports in its kind's index order: the inputs, then the outputs.
    ports: Vec<Vec<PortInfo>>,
    failed: Vec<bool>,
    bypassed: Vec<bool>,
    wires: Vec<Wire>,
    /// The reference of each wire, in `wires` order.
    refs: Vec<WireRef>,
    /// Every node and every frame with its title: what Fit on Screen shows.
    extent: DocRect,
}

/// The material graph's nodes and wires for this pass, or nothing when no graph is open.
fn node_list(app: &MaterialCraftApp, ctx: &egui::Context, layers: &AnnotationLayers) -> Option<NodeList> {
    let graph = app.session.graph().ok()?;
    let index = materialcraft_engine::core::GraphIndex::new(graph);
    let carried = layers.carried();
    let mut list = NodeList::default();
    let mut bounds = Rect::NOTHING;
    for node in &graph.nodes {
        let (ins, outs) = materialcraft_engine::eval::ports_with_connections(&app.session.pkg, node, index.incoming(node.id).iter().copied());
        let ports = ins.iter().map(|port| (port.id.clone(), port.label.clone(), port.kind, false));
        let ports: Vec<PortInfo> = ports.chain(outs.iter().map(|port| (port.id.clone(), port.label.clone(), port.kind, true))).collect();
        let kind = MaterialKind::new(app, ctx, node, ins, outs);
        let mut pos = node.pos;
        if let Some(by) = app.graph.node_move.filter(|by| by.length() > 0.5 && app.session.selection.contains(&node.id)) {
            pos = materialcraft_engine::graph_grid::translate(pos, by.into(), app.ui.prefs.snap_to_grid);
        }
        let mut rect = kind.rect(pos);
        if let Some((_, by)) = carried.filter(|(contents, _)| contents.nodes.contains(&node.id)) {
            rect = rect.translate(by);
        }
        bounds = bounds.union(rect);
        list.ids.push(node.id);
        list.positions.push(rect);
        list.kinds.push(kind);
        list.ports.push(ports);
        list.failed.push(app.session.cooked.nodes.get(&node.id).is_some_and(|result| result.error.is_some()));
        list.bypassed.push(node.disabled);
    }
    for frame in &graph.frames {
        let [x, y, w, h] = frame.rect;
        bounds = bounds.union(Rect::from_min_size(pos2(x, y - FRAME_TITLE), vec2(w, h + FRAME_TITLE)));
    }
    list.extent = if bounds.is_positive() { doc_rect(bounds) } else { DocRect::ZERO };

    let mut at = HashMap::new();
    for (place, id) in list.ids.iter().enumerate() {
        at.entry(*id).or_insert(place);
    }
    for connection in &graph.connections {
        let (Some(&from), Some(&to)) = (at.get(&connection.from), at.get(&connection.to)) else { continue };
        let port_of = |node: usize, id: &str, output: bool| list.ports.get(node)?.iter().position(|port| port.3 == output && port.0 == id);
        let (Some(from_port), Some(to_port)) = (port_of(from, &connection.from_port, true), port_of(to, &connection.to_port, false)) else { continue };
        let wire: WireRef = (connection.to, connection.to_port.clone());
        let ends = (PortRef::new(GraphNodeId::new(from as u32), from_port), PortRef::new(GraphNodeId::new(to as u32), to_port));
        list.wires.push(Wire::new(wire_id(&wire), ends.0, ends.1));
        list.refs.push(wire);
    }
    Some(list)
}

/// A rect in graph units as the canvas's document rect.
fn doc_rect(rect: Rect) -> DocRect {
    DocRect::new(f64::from(rect.min.x), f64::from(rect.min.y), f64::from(rect.max.x), f64::from(rect.max.y))
}

/// The screen point of the graph point `at` through `view`.
fn to_screen(view: &CanvasView, at: Pos2) -> Pos2 {
    view.to_screen(Point::new(f64::from(at.x), f64::from(at.y)))
}

/// The material graph on a `Canvas`: the canvas pans and zooms with no edges, and the framework node graph inside it
/// draws the nodes, wires and annotations and reports the gestures, which `canvas_events` maps onto commands. The
/// body runs twice when the canvas measures first; only the run with a canvas space (the real one) returns the
/// graph's response, so the events, the publish and the queue drop happen once per pass.
fn canvas(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if app.ui.frame_all || app.graph.framed_graph != Some(app.session.graph) {
        app.ui.frame_all = false;
        app.graph.framed_graph = Some(app.session.graph);
        app.graph.selected_wires = Selection::default();
        app.graph.request_view(ViewRequest::Place(ViewCommands::FitOnScreen));
    }

    let ctx = ui.ctx().clone();
    let pixels_per_point = ctx.pixels_per_point();
    let layers = annotations::layers(app);
    let Some(list) = node_list(app, &ctx, &layers) else { return };
    let (backdrops, notes, markers) = (layers.backdrops(), layers.notes(), layers.markers());
    let kinds: Vec<&dyn NodeKind<()>> = list.kinds.iter().map(|kind| kind as &dyn NodeKind<()>).collect();
    let graph_ids: Vec<GraphNodeId> = (0..list.ids.len() as u32).map(GraphNodeId::new).collect();
    let source = keyed(&graph_ids, |id| *id);
    let selection = canvas_events::given_selection(&app.session.selection, &list.ids);
    let mut wire_selection = app.graph.selected_wires.clone();
    let queued = app.graph.pending_views.clone();
    let given = app.ui.graph_camera.view(app.graph.view);
    let target = ViewTarget::unbounded(list.extent).fit_ceiling(FIT_CAP * f64::from(pixels_per_point));
    // Fit Layers (F) shows the selected nodes, else the whole graph.
    let picked =
        list.ids.iter().zip(&list.positions).filter(|(id, _)| app.session.selection.contains(*id)).fold(Rect::NOTHING, |all, (_, rect)| all.union(*rect));
    let layer_bounds = || Some(if picked.is_positive() { doc_rect(picked) } else { list.extent });
    let mut navigation = NavigationOptions::default();
    navigation.wheel = WheelScheme::Zoom;
    navigation.pinch_zooms = true;
    navigation.invert_wheel_zoom = app.ui.prefs.invert_zoom;

    let shown = with_env(
        ui,
        |scope| {
            scope.values.set(graph_zoom(pixels_per_point));
        },
        |ui| {
            Canvas::new(tr("Graph")).document(target).layer_bounds(&layer_bounds).queued(&queued).navigation(navigation).view(Bind::given(given)).show(
                ui,
                |ui, body| {
                    let space = canvas_space(&body.env)?;
                    let _ =
                        CanvasGrid::new(tr("Grid")).spacing(DocVec::new(GRID_STEP, GRID_STEP)).subdivisions(GRID_SUBDIVISIONS).picture().show_at(ui, body.rect);
                    let mut graph = NodeGraph::new(&list.positions, &kinds, &list.wires)
                        .label(tr("Node graph"))
                        .command_scope(GRAPH_SCOPE)
                        .selection(Bind::given(selection.clone()))
                        .wire_selection(Bind::bound(&mut wire_selection))
                        .backdrops(&backdrops)
                        .notes(&notes)
                        .markers(&markers)
                        .annotation_selection(Bind::given(layers.selected()))
                        .takes_drop(DropKind::new(DropPlace::Spot, |_: &GraphDragPayload, _| true))
                        .takes_drop(DropKind::new(DropPlace::Spot, |target: &ExplorerTarget, _| matches!(target, ExplorerTarget::Resource { .. })))
                        .takes_drop(DropKind::new(DropPlace::Spot, |_: &LibraryDrag, _| true));
                    if let Some(transform) = space.view.layer_transform() {
                        graph = graph.zoom(transform);
                    }
                    let response: NodeGraphResponse<()> = graph.show_items(ui, &source, &mut |id, _| {
                        let at = id.0 as usize;
                        let node = Node::new(*id, kinds[at]);
                        node.failed(list.failed.get(at).copied().unwrap_or(false)).bypassed(list.bypassed.get(at).copied().unwrap_or(false))
                    });
                    Some((response, space.view))
                },
            )
        },
    );

    let canvas_rect = shown.output.response.rect;
    app.ui.graph_camera = GraphCamera::from_view(&shown.output.view.unwrap_or(given));
    if let Some((response, view)) = shown.inner.flatten() {
        app.graph.pending_views.clear();
        app.graph.selected_wires = wire_selection;
        publish(app, &list, &view, &response.annotation_rects);
        app.graph.inspected_node = super::node_selection::inspected(&app.session.selection, app.graph.inspected_node);
        canvas_events::apply(app, ui, &response, &list.ids);
        app.graph.inspected_node = super::node_selection::inspected(&app.session.selection, app.graph.inspected_node);
    }

    super::search::show(app, ui, SearchEditor::Material);
    footer(app, ui, canvas_rect);
}

/// Stores what the canvas showed this pass in screen points, for `ui.inspect`, scripted input and the event mapping:
/// the node cards, each node's ports in its kind's order, the wires' control points and the annotations' rects.
fn publish(app: &mut MaterialCraftApp, list: &NodeList, view: &CanvasView, annotation_rects: &[AnnotationRect]) {
    let screen_rect = |rect: &Rect| Rect::from_two_pos(to_screen(view, rect.min), to_screen(view, rect.max));
    let rects = list.ids.iter().zip(&list.positions).map(|(id, rect)| (*id, screen_rect(rect))).collect();
    let mut ports = Vec::new();
    for (((id, rect), kind), node_ports) in list.ids.iter().zip(&list.positions).zip(&list.kinds).zip(&list.ports) {
        for (index, (port, label, value, output)) in node_ports.iter().enumerate() {
            let pos = to_screen(view, rect.min + kind.port_offset(index, rect.size()));
            ports.push(PortHit { node: *id, output: *output, port: port.clone(), label: label.clone(), pos, kind: *value });
        }
    }
    let end = |port: PortRef| {
        let (rect, kind) = (list.positions.get(port.node.0 as usize)?, list.kinds.get(port.node.0 as usize)?);
        Some((rect.min + kind.port_offset(port.port, rect.size()), kind.port_side(port.port)))
    };
    let wires = list
        .wires
        .iter()
        .zip(&list.refs)
        .filter_map(|(wire, reference)| {
            let ((from, from_side), (to, to_side)) = (end(wire.from)?, end(wire.to)?);
            Some((reference.clone(), wire_controls(from, from_side, to, to_side).map(|at| to_screen(view, at))))
        })
        .collect();
    app.graph.publish(view.rect, rects, ports, wires, annotations::screen_rects(annotation_rects));
}

/// The failed and missing node counts at the canvas's bottom end corner, failures lowest; captions over the view,
/// which sense nothing, so every press stays the canvas's.
fn footer(app: &MaterialCraftApp, ui: &mut Ui, rect: Rect) {
    let (failed, placeholders) = app.session.cook_failures();
    if failed.is_empty() && placeholders == 0 {
        return;
    }
    let env = Env::of(ui);
    let (inset, gap) = (env.tokens.space.pad.regular, env.tokens.space.gap.tight);
    let mut corner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(inset)).layout(egui::Layout::bottom_up(egui::Align::Max)));
    corner.spacing_mut().item_spacing.y = gap;
    if !failed.is_empty() {
        let text = format!("{} {} {}", icon::WARNING, failed.len(), tr("node(s) failed"));
        Text::new(text).role(TypeRole::Caption).tone(Tone::Bad).show(&mut corner);
    }
    // Placeholders for missing SBS nodes are a soft warning, not a failure
    if placeholders > 0 {
        let text = format!("{} {placeholders} {}", icon::WARNING, tr("missing node(s), see the Console"));
        Text::new(text).role(TypeRole::Caption).tone(Tone::Warn).show(&mut corner);
    }
}
