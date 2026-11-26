use super::FunctionEditor;
use super::node_kind::FunctionValueEdit;
use crate::{
    MaterialCraftApp,
    graph::{add_node_menu, edit_menu, wire_actions},
    i18n::tr,
};
use craft_ui::contract::ShowContainer as _;
use egui::Ui;
use materialcraft_engine::SelectionTarget;
use serde_json::json;

pub(super) fn show(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, ui: &mut Ui, graph: &craft_ui::graph::NodeGraphResponse<FunctionValueEdit>, ids: &[u32]) {
    let rows = graph
        .collection
        .menu
        .as_ref()
        .and_then(|request| ids.get(request.key.0 as usize).map(|id| node_actions(app, editor, *id)))
        .unwrap_or_else(|| background_actions(app, editor));
    if let Some(request) = graph.collection.menu.as_ref() {
        let _ = craft_ui::menu::Menu::context(tr("Function graph"), &graph.collection.response)
            .anchor(craft_ui::contract::overlay::Anchor::Point(request.at))
            .show(ui, |ui, _| craft_menu::context::rows(ui, &rows));
    }
}

/// The Add Comment and Add Frame rows at the menu's point.
fn annotations(editor: &FunctionEditor) -> [craft_menu::Node; 2] {
    let at = editor.menu_at;
    [
        edit_menu::row("ui.function.annotate", "Add Comment", json!({"command":"graph.comment.add", "params":{"at":[at.x, at.y]}})),
        edit_menu::row("ui.function.annotate", "Add Frame", json!({"command":"graph.frame.add", "params":{"rect":[at.x, at.y, 400, 260]}})),
    ]
}

fn node_actions(app: &MaterialCraftApp, editor: &FunctionEditor, node: u32) -> Vec<craft_menu::Node> {
    let mut rows = Vec::from(annotations(editor));
    rows.push(edit_menu::row("ui.function.copyInfo", "Copy info to clipboard", json!({"id":node})));
    if editor.current(app).and_then(|g| g.node(node)).is_some_and(|n| n.instance.is_some()) {
        rows.push(edit_menu::row("ui.function.openReference", "Open reference", json!({"id":node})));
    }
    if let (Some(graph), Some(target)) = (editor.current(app), editor.command_target())
        && graph.node(node).is_some_and(|n| materialcraft_engine::nodes::function::can_be_output(&n.op))
    {
        let enabled = editor.selection.len() == 1
            && graph.output != Some(node)
            && app.session.graph().ok().is_some_and(|doc| materialcraft_engine::function_output::validate(doc, &target, graph, node).is_ok());
        let row = edit_menu::row("ui.function.setOutput", "Set as output node", json!({"id":node}));
        rows.push(if enabled { row } else { row.enabled(false) });
    }
    rows.extend(edit_menu::selection_rows(editor.selection_parameters("graph.createFromSelection")));
    rows
}

fn background_actions(app: &MaterialCraftApp, editor: &FunctionEditor) -> Vec<craft_menu::Node> {
    let Some(target) = editor.command_target() else { return Vec::new() };
    let at = editor.menu_at;
    let entries = target.available_operations(&app.session).filter(|op| op.id != "passthrough").map(|op| (op.id, op.label, ""));
    let mut rows = add_node_menu::rows("ui.function.add", entries, |id| json!({"operation":id, "at":[at.x, at.y], "snap":app.ui.prefs.snap_to_grid}));
    rows.extend(annotations(editor));
    rows.push(edit_menu::row("ui.function.annotate", "Add Navigation pin", json!({"command":"graph.pin.add", "params":{"at":[at.x, at.y]}})));
    rows.push(edit_menu::row("ui.function.add", "Add Dot node", json!({"operation":"passthrough", "at":[at.x, at.y], "snap":app.ui.prefs.snap_to_grid})));
    rows.push(edit_menu::row("file.save", "Save package", json!({})));
    rows.extend(edit_menu::selection_rows(editor.selection_parameters("edit.selectAll")));
    rows.push(edit_menu::row("ui.graph.previousPin", "Previous pin", json!({})));
    rows.push(edit_menu::row("ui.graph.nextPin", "Next pin", json!({})));
    if let Some(row) = wire_actions::swap_row(app, Some(SelectionTarget::Function(target))) {
        rows.push(row);
    }
    rows
}
