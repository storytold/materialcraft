use materialcraft_engine::core::NodeId;
use serde_json::{Value, json};

use crate::MaterialCraftApp;

fn item(command: &str, label: &str, params: Value) -> craft_menu::Node {
    craft_menu::Node::Item(craft_menu::Item::new(craft_keymap::CommandRef::with_params(command, params), crate::i18n::tr(label)))
}

/// Builds context rows for a graph instance, including the selection commands supported there.
pub(super) fn rows(app: &MaterialCraftApp, node: NodeId, at: egui::Pos2) -> Vec<craft_menu::Node> {
    let mut rows = super::context_menu::instance_output_rows(app, node);
    rows.extend(crate::material_preview_menu::rows(app, app.session.graph, node));
    rows.extend([
        craft_menu::Node::Separator,
        item("graph.comment.add", "Add Comment", json!({"at": [at.x, at.y]})),
        item("graph.frame.add", "Add Frame", json!({"rect": [at.x, at.y, 400.0, 260.0]})),
        item("ui.graph.copyInfo", "Copy info to clipboard", json!({"id": node.0})),
        craft_menu::Node::Separator,
        craft_menu::Node::submenu(crate::i18n::tr("Manage parameters"), vec![item("graph.node.reset", "Reset parameters", json!({"id": node.0}))]),
        craft_menu::Node::Separator,
    ]);
    let point = json!([at.x, at.y]);
    let catalog = materialcraft_engine::nodes::catalog()
        .iter()
        .filter(|definition| definition.id != "dot")
        .map(|definition| (definition.id, definition.label, if definition.id == "output" { "Output" } else { definition.category }));
    rows.push(craft_menu::Node::submenu(
        crate::i18n::tr("Create"),
        vec![
            craft_menu::Node::submenu(
                crate::i18n::tr("Add node"),
                super::add_node_menu::rows("graph.node.add", catalog, |id| json!({"def": id, "at": point, "snapToGrid": app.ui.prefs.snap_to_grid})),
            ),
            item("graph.node.add", "Add Dot node", json!({"def": "dot", "at": point})),
            item("graph.pin.add", "Add Navigation pin", json!({"at": point})),
        ],
    ));
    rows.extend([
        craft_menu::Node::Separator,
        item("graph.node.edit", "Open reference", Value::Null),
        craft_menu::Node::Separator,
        item("graph.createFromSelection", &format!("{} ({})", crate::i18n::tr("Create graph from selection"), app.session.selection.len()), Value::Null),
    ]);
    let commands = [
        ("Copy selection", "edit.copy"),
        ("Delete selection", "edit.delete"),
        ("Delete and relink", "edit.deleteRelink"),
        ("Duplicate selection", "edit.duplicate"),
        ("Duplicate selection without link", "edit.duplicateWithoutLinks"),
        ("Select upstream nodes", "graph.selectUpstream"),
        ("Select downstream nodes", "graph.selectDownstream"),
        ("Disable selection", "graph.node.disable"),
    ];
    rows.extend(commands.into_iter().map(|(label, command)| item(command, label, Value::Null)));
    if let Some(row) = super::wire_actions::swap_row(app, None) {
        rows.push(row);
    }
    rows
}
