use egui::{Pos2, Ui};
use materialcraft_engine::core::NodeId;
use serde_json::{Value, json};

use crate::MaterialCraftApp;

fn item(command: &str, label: &str, params: Value) -> craft_menu::Node {
    craft_menu::Node::Item(craft_menu::Item::new(craft_keymap::CommandRef::with_params(command, params), crate::i18n::tr(label)))
}

fn submenu(label: &str, children: Vec<craft_menu::Node>) -> craft_menu::Node {
    craft_menu::Node::submenu(crate::i18n::tr(label), children)
}

/// Shows graph context rows from a collection context request or an empty canvas request.
pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, graph: &craft_ui::graph::NodeGraphResponse<()>, nodes: &[NodeId]) {
    let request = graph.collection.menu.as_ref();
    let event_at = graph.collection.events.iter().rev().find_map(|event| match event {
        craft_ui::contract::CollectionEvent::ContextMenu(request) => Some((Some(request.key), request.at)),
        craft_ui::contract::CollectionEvent::EmptyContextMenu { at } => Some((None, *at)),
        _ => None,
    });
    let target_index = event_at.and_then(|(node, _)| node).or_else(|| request.map(|request| request.key));
    let target = target_index.and_then(|key| nodes.get(key.0 as usize).copied());
    if let Some(node) = target
        && app.session.selection.last() != Some(&node)
    {
        let mut selection = app.session.selection.clone();
        if selection.contains(&node) {
            selection.retain(|selected| *selected != node);
        } else {
            selection.clear();
        }
        selection.push(node);
        app.run("graph.select", json!({"ids": selection}));
    }
    let screen = event_at.map(|(_, at)| at).or_else(|| request.map(|request| request.at)).unwrap_or(app.graph.view.min);
    let view = crate::annotation::camera(app).view(app.graph.view);
    let point = view.to_doc(screen);
    let at = Pos2::new(point.x as f32, point.y as f32);
    let instance = target.is_some_and(|id| app.session.graph().ok().and_then(|graph| graph.node(id).ok()).is_some_and(|node| node.def == "graph_instance"));
    let rows = if instance { target.map_or_else(Vec::new, |node| super::instance_context_menu::rows(app, node, at)) } else { rows(app, target, at) };
    let menu = craft_ui::menu::Menu::context(crate::i18n::tr("Graph context menu"), &graph.collection.response);
    let _ = menu.show(ui, |ui, _| craft_menu::context::rows(ui, &rows));
}

fn rows(app: &MaterialCraftApp, node: Option<NodeId>, at: Pos2) -> Vec<craft_menu::Node> {
    let point = json!([at.x, at.y]);
    let mut rows = Vec::new();
    if node.is_some() {
        rows.push(item("graph.node.edit", "Edit graph", Value::Null));
        rows.push(craft_menu::Node::Separator);
    }
    let entries = materialcraft_engine::nodes::catalog()
        .iter()
        .filter(|definition| definition.id != "dot")
        .map(|definition| (definition.id, definition.label, if definition.id == "output" { "Output" } else { definition.category }));
    rows.push(submenu(
        "Add node",
        super::add_node_menu::rows("graph.node.add", entries, |id| json!({"def": id, "at": point, "snapToGrid": app.ui.prefs.snap_to_grid})),
    ));
    rows.extend([
        item("graph.comment.add", "Add Comment", json!({"at": point})),
        item("graph.frame.add", "Add Frame", json!({"rect": [at.x, at.y, 400.0, 260.0]})),
        item("graph.pin.add", "Add Navigation pin", json!({"at": point})),
        item("graph.node.add", "Add Dot node", json!({"def": "dot", "at": point})),
        craft_menu::Node::Separator,
    ]);
    if let Some(node) = node {
        rows.extend(crate::material_preview_menu::rows(app, app.session.graph, node));
    } else {
        rows.push(item("ui.graph.viewOutputs", "View outputs in 3D view", Value::Null));
        rows.push(item("ui.graph.resetViewOutputs", "Reset and view outputs in 3D view", Value::Null));
    }
    rows.extend(output_rows(app, node));
    rows.extend([
        item("ui.graph.computeThumbnails", "Compute nodes thumbnails", Value::Null),
        item("ui.graph.clearThumbnails", "Clear nodes thumbnails", Value::Null),
        item("file.save", "Save package", Value::Null),
    ]);
    rows.extend(super::edit_menu::selection_rows(json!({"at": point})));
    if let Some(row) = super::wire_actions::swap_row(app, None) {
        rows.push(row);
    }
    rows.extend([item("ui.graph.previousPin", "Previous pin", Value::Null), item("ui.graph.nextPin", "Next pin", Value::Null)]);
    rows
}

pub(super) fn instance_output_rows(app: &MaterialCraftApp, node: NodeId) -> Vec<craft_menu::Node> {
    output_rows(app, Some(node))
}

fn output_rows(app: &MaterialCraftApp, node: Option<NodeId>) -> Vec<craft_menu::Node> {
    let Some(graph) = app.session.graph().ok() else { return Vec::new() };
    let targets: Vec<_> = graph
        .nodes
        .iter()
        .filter(|candidate| node == Some(candidate.id) || (node.is_none() && candidate.def == "output"))
        .flat_map(|candidate| {
            if candidate.def == "output" {
                return vec![(candidate.id, 0, super::node_label(candidate))];
            }
            materialcraft_engine::eval::ports(&app.session.pkg, graph, candidate)
                .1
                .into_iter()
                .enumerate()
                .map(|(port, output)| (candidate.id, port, format!("{} / {}", super::node_label(candidate), output.label)))
                .collect()
        })
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    vec![submenu(
        "View output in 2D view",
        targets.into_iter().map(|(node, port, label)| item("ui.graph.view2d", &label, json!({"node": node.0, "port": port}))).collect(),
    )]
}
