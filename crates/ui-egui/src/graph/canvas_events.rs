//! Maps the framework node graph's events onto commands.
//!
//! The graph reports a press, a drag start and a release once, in the pass that applies that input, so a second pass
//! in the frame brings no events and runs nothing again; the menus below are shown every pass, as `Menu::context` asks.

use craft_behavior::kurbo::Point;
use craft_behavior::{EditEvent, Selection, SelectionChange, SelectionModel as _};
use craft_ui::contract::{CollectionEvent, DropAt, DropPayload};
use craft_ui::graph::{NodeEvent, NodeGraphResponse, NodeId as GraphNodeId, PortRef, WireEvent};
use egui::{Pos2, Ui, Vec2, pos2};
use materialcraft_engine::core::NodeId;
use serde_json::json;

use super::editor_target::EditorTarget;
use super::node_search::SearchWire;
use super::{LibraryDrag, NODE_W, WireRef, wire_id};
use crate::MaterialCraftApp;
use crate::explorer::ExplorerTarget;
use crate::graph_drag_payload::GraphDragPayload;

/// How close to a port, in screen points, a reverse wire's release counts as aimed at it: a release there opens no
/// search (`input.rs:161` of the replaced dispatch).
const PORT_REACH: f32 = 12.0;

/// The framework selection of the session's `selected` nodes: `NodeId(i)` is the node at position `i` of `nodes`, the
/// one list the pass builds from `graph.nodes`. The last selected node is the lead, the first the anchor.
pub(super) fn given_selection(selected: &[NodeId], nodes: &[NodeId]) -> Selection<GraphNodeId> {
    let items: Vec<GraphNodeId> =
        selected.iter().filter_map(|id| nodes.iter().position(|node| node == id)).map(|at| GraphNodeId::new(at as u32)).collect();
    let (lead, anchor) = (items.last().copied(), items.first().copied());
    Selection::new(items, lead, anchor)
}

/// Maps the events, calls `annotations::apply`, `context_menu::show`, `wire_actions::wire_menu` and the node search
/// open; `nodes` is the pass's node list the graph's `NodeId`s index.
pub(crate) fn apply(app: &mut MaterialCraftApp, ui: &mut Ui, response: &NodeGraphResponse<()>, nodes: &[NodeId]) {
    let node = |id: &GraphNodeId| nodes.get(id.0 as usize).copied();
    for event in &response.collection.events {
        match event {
            CollectionEvent::Selection { change, .. } => select(app, nodes, change.clone()),
            CollectionEvent::Move(edit) => moved(app, edit),
            CollectionEvent::Activate(id) => {
                if let Some(id) = node(id) {
                    activate(app, id);
                }
            }
            CollectionEvent::EmptyActivated { at, .. } => {
                let at = to_graph(app, *at);
                super::search::open(app, EditorTarget::Material, at, None);
            }
            CollectionEvent::ContextMenu(request) => menu_at(app, request.at),
            CollectionEvent::EmptyContextMenu { at, .. } => menu_at(app, *at),
            CollectionEvent::Dropped { at: DropAt::Spot { at, .. }, payload, .. } => dropped(app, *at, payload),
            CollectionEvent::Item(id, event) => {
                if let Some(id) = node(id) {
                    item(app, nodes, id, event);
                }
            }
            _ => {}
        }
    }
    for event in &response.wires {
        wire(app, *event);
    }
    super::annotations::apply(app, &response.annotations);

    match app.graph.menu_wire.clone() {
        Some(wire) => {
            let at = app.graph.menu_at;
            super::wire_actions::wire_menu(app, ui, &response.collection.response, wire, at);
        }
        None => super::context_menu::show(app, ui, response, nodes),
    }
}

/// Applies a selection change to the session's nodes, as the replaced dispatch wrote them (`input.rs:75, :130`).
fn select(app: &mut MaterialCraftApp, nodes: &[NodeId], change: SelectionChange<GraphNodeId>) {
    let mut selection = given_selection(&app.session.selection, nodes);
    selection.apply(change);
    app.session.selection = selection.items.iter().filter_map(|id| nodes.get(id.0 as usize).copied()).collect();
    app.graph.inspected_node = super::node_selection::inspected(&app.session.selection, app.graph.inspected_node);
}

/// The node move: its preview offset while it runs, one `graph.node.move` when it commits past half a unit
/// (`input.rs:123-126`), nothing when it is cancelled.
fn moved(app: &mut MaterialCraftApp, edit: &EditEvent<Vec2>) {
    match edit {
        EditEvent::Begin { .. } => app.graph.node_move = Some(Vec2::ZERO),
        EditEvent::Update { value, .. } => app.graph.node_move = Some(*value),
        EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => {
            app.graph.node_move = None;
            if after.length() > 0.5 {
                app.run("graph.node.move", json!({"by": [after.x, after.y]}));
            }
        }
        EditEvent::Cancel { .. } => app.graph.node_move = None,
        _ => {}
    }
}

/// A double click on a node previews its first output; an embedded graph opens only through its own edit action.
fn activate(app: &mut MaterialCraftApp, id: NodeId) {
    if app.session.graph().ok().and_then(|graph| graph.node(id).ok()).is_some_and(|node| node.def != "graph_instance") {
        app.run("view.2d", json!({"node": id.0, "port": 0}));
    }
}

/// The graph point of a menu opened at the screen point `at`, for the rows that add there; a node's or the canvas's
/// menu is no wire's.
fn menu_at(app: &mut MaterialCraftApp, at: Pos2) {
    app.graph.menu_at = to_graph(app, at);
    app.graph.menu_wire = None;
}

/// What a node reported: a wire made, let go or detached, and clicks on its body and its ports.
fn item(app: &mut MaterialCraftApp, nodes: &[NodeId], id: NodeId, event: &NodeEvent<()>) {
    match event {
        NodeEvent::Wired { from, to, detached, .. } => {
            // The graph orders the ends (output first), a reverse wire's included.
            let (Some((from, true)), Some((to, false))) = (port_ref(app, nodes, *from), port_ref(app, nodes, *to)) else { return };
            let mut params = json!({"from": from.0.0, "fromPort": from.1, "to": to.0.0, "toPort": to.1});
            if let Some((node, port)) = detached.and_then(|wire| wire_ref(app, wire)) {
                params["detach"] = json!({"to": node.0, "toPort": port});
            }
            app.run("graph.connect", params);
        }
        NodeEvent::WireEnded { from, detached, loose: Some(at), .. } => match detached.and_then(|wire| wire_ref(app, wire)) {
            Some((node, port)) => {
                app.run("graph.disconnect", json!({"to": node.0, "toPort": port}));
            }
            None => loose_end(app, nodes, *from, *at),
        },
        NodeEvent::Clicked { port: None, .. } => {
            // A click on the node already shown, or one that deselected it, keeps the current preview.
            if app.session.selection.contains(&id) && app.session.view2d.node != Some(id) {
                app.run("view.2d", json!({"node": id.0}));
            }
        }
        NodeEvent::Clicked { port: Some(index), .. } => {
            let Some((port, true)) = port(app, id, *index) else { return };
            let index = app.session.output_ports(id).iter().position(|output| *output == port).unwrap_or(0);
            app.run("view.2d", json!({"node": id.0, "port": index}));
        }
        _ => {}
    }
}

/// A wire let go where no port took it: from an output, the node search for a node it feeds; from an input (a
/// reverse wire), the search for a node whose output feeds it, unless the release was aimed at a port that refused
/// it. Both only inside the canvas and with the preference on.
fn loose_end(app: &mut MaterialCraftApp, nodes: &[NodeId], from: PortRef, at: Pos2) {
    let Some((end, output)) = port_ref(app, nodes, from) else { return };
    let screen = to_screen(app, at);
    if !app.ui.prefs.wire_drop_opens_node_search || !app.graph.view.contains(screen) {
        return;
    }
    // PLAN: the sheet (note 7e) names `From` for a drag from an output and `To` for a reverse wire; `SearchWire`'s own
    // docs (graph-w11, `node_search.rs:7-10`) say `From` holds the input the new node feeds and `To` the output that
    // feeds it, so the ends follow those docs and the behaviour stays the sheet's.
    let wire = if output {
        SearchWire::To(end)
    // PLAN: graph-w10's sheet removes `port_hit::nearest`; this 12-point refusal (sheet note 7e) is its one reader,
    // so graph-w10 keeps it.
    } else if super::port_hit::nearest(&app.graph.ports, screen, PORT_REACH).is_none() {
        SearchWire::From(end)
    } else {
        return;
    };
    super::search::open(app, EditorTarget::Material, at, Some(wire));
}

/// What a wire reported: a double click and a pull insert a reroute dot, a secondary press opens the wire's menu.
fn wire(app: &mut MaterialCraftApp, event: WireEvent) {
    match event {
        WireEvent::DoubleClicked { wire, at } | WireEvent::Pulled { wire, at } => {
            // `graph.wire.insertDot` selects the dot it adds (`wire_cmds.rs:134`), so the pull's move carries it.
            if let Some(wire) = wire_ref(app, wire) {
                app.run("graph.wire.insertDot", json!({"to": wire.0.0, "toPort": wire.1, "at": [at.x, at.y]}));
                app.graph.selected_wires = Selection::default();
            }
        }
        WireEvent::ContextMenu { wire, at } => {
            app.graph.menu_wire = wire_ref(app, wire);
            app.graph.menu_at = to_graph(app, at);
        }
        _ => {}
    }
}

/// A drop from the Explorer or the library at the graph point `at`: the dropped item's card lands with its top
/// centre near the point.
fn dropped(app: &mut MaterialCraftApp, at: Pos2, payload: &DropPayload) {
    let corner = [at.x - NODE_W * 0.5, at.y - 20.0];
    if let Some(graph) = payload.get::<GraphDragPayload>() {
        app.run("graph.instance.add", json!({"document": graph.document, "graph": graph.name, "at": corner}));
    } else if let Some(ExplorerTarget::Resource { document, name }) = payload.get::<ExplorerTarget>() {
        if app.run("graph.resource.add", json!({"document": document, "resource": name, "at": corner})).is_some() {
            app.inspected_folder = None;
            app.inspected_graph = None;
        }
    } else if let Some(entry) = payload.get::<LibraryDrag>() {
        app.run("library.add", json!({"entry": entry.0, "at": corner, "snapToGrid": app.ui.prefs.snap_to_grid}));
    }
}

/// Port `index` of `node` and whether it is an output, from the ports the canvas published this pass, which list
/// each node's inputs and then its outputs: the index scheme of the node's kind. Node 7 with inputs `a`, `b` and
/// output `o` publishes `a`, `b`, `o`, so index 2 is `o`, an output.
fn port(app: &MaterialCraftApp, node: NodeId, index: usize) -> Option<(String, bool)> {
    app.graph.ports.iter().filter(|hit| hit.node == node).nth(index).map(|hit| (hit.port.clone(), hit.output))
}

/// The node and port id `at` names, through the pass's node list, and whether the port is an output.
fn port_ref(app: &MaterialCraftApp, nodes: &[NodeId], at: PortRef) -> Option<(WireRef, bool)> {
    let node = nodes.get(at.node.0 as usize).copied()?;
    port(app, node, at.port).map(|(port, output)| ((node, port), output))
}

/// The wire of id `id` among the wires the canvas published this pass.
fn wire_ref(app: &MaterialCraftApp, id: u64) -> Option<WireRef> {
    app.graph.wires.iter().map(|(wire, _)| wire).find(|wire| wire_id(wire) == id).cloned()
}

/// The graph point under the screen point `at`, through the view the canvas showed this pass.
fn to_graph(app: &MaterialCraftApp, at: Pos2) -> Pos2 {
    let point = app.ui.graph_camera.view(app.graph.view).to_doc(at);
    pos2(point.x as f32, point.y as f32)
}

/// The screen point of the graph point `at`, through the view the canvas showed this pass.
fn to_screen(app: &MaterialCraftApp, at: Pos2) -> Pos2 {
    app.ui.graph_camera.view(app.graph.view).to_screen(Point::new(f64::from(at.x), f64::from(at.y)))
}
