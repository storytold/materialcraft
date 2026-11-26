use super::FunctionEditor;
use crate::{
    MaterialCraftApp,
    graph::{add_node_menu, edit_menu, wire_actions},
    i18n::tr,
};
use egui::Ui;
use materialcraft_engine::{FunctionEdit, SelectionTarget};
use serde_json::json;

pub(super) fn show(
    app: &mut MaterialCraftApp,
    editor: &mut FunctionEditor,
    ui: &mut Ui,
    graph: &craft_ui::graph::NodeGraphResponse<super::canvas::FunctionValueEdit>,
    ids: &[u32],
) {
    let rows = graph.collection.menu.as_ref().and_then(|request| {
        ids.get(request.key.0 as usize).map(|id| node_actions(app, editor, *id))
    }).unwrap_or_else(|| background_actions(app, editor));
    if let Some(request) = graph.collection.menu.as_ref() {
        let _ = craft_ui::menu::Menu::context(tr("Function graph"), &graph.collection.response)
            .anchor(craft_ui::contract::overlay::Anchor::Point(request.at))
            .show(ui, |ui, _| craft_menu::context::rows(ui, &rows));
    }
}

fn node_actions(app: &MaterialCraftApp, editor: &FunctionEditor, node: u32) -> Vec<craft_menu::Node> {
    let mut rows = vec![
        edit_menu::row("ui.function.annotate", json!({"command":"graph.comment.add", "params":{"at":[editor.menu_at.x, editor.menu_at.y]}})),
        edit_menu::row("ui.function.annotate", json!({"command":"graph.frame.add", "params":{"rect":[editor.menu_at.x, editor.menu_at.y, 400, 260]}})),
        edit_menu::row("ui.function.copyInfo", json!({"id":node})),
    ];
    if editor.current(app).and_then(|g| g.node(node)).is_some_and(|n| n.instance.is_some()) {
        rows.push(edit_menu::row("ui.function.openReference", json!({"id":node})));
    }
    if let (Some(graph), Some(target)) = (editor.current(app), editor.command_target())
        && graph.node(node).is_some_and(|n| materialcraft_engine::nodes::function::can_be_output(&n.op))
    {
        let enabled = editor.selection.len() == 1
            && graph.output != Some(node)
            && app.session.graph().ok().is_some_and(|doc| materialcraft_engine::function_output::validate(doc, &target, graph, node).is_ok());
        rows.push(if enabled {
            edit_menu::row("ui.function.setOutput", json!({"id":node}))
        } else {
            edit_menu::row("ui.function.setOutput", json!({"id":node})).enabled(false)
        });
    }
    let command = "graph.createFromSelection";
    let _ = app;
    rows.extend(edit_menu::selection_rows(editor.selection_parameters(command)));
    rows
}

fn background_actions(app: &MaterialCraftApp, editor: &FunctionEditor) -> Vec<craft_menu::Node> {
    let Some(target) = editor.command_target() else { return Vec::new() };
    let entries = target.available_operations(&app.session).filter(|op| op.id != "passthrough").map(|op| (op.id, op.label, ""));
    let mut rows = add_node_menu::rows(
        "ui.function.add",
        entries,
        |id| json!({"operation":id, "at":[editor.menu_at.x, editor.menu_at.y], "snap":app.ui.prefs.snap_to_grid}),
    );
    rows.push(edit_menu::row("ui.function.annotate", json!({"command":"graph.comment.add", "params":{"at":[editor.menu_at.x, editor.menu_at.y]}})));
    rows.push(edit_menu::row("ui.function.annotate", json!({"command":"graph.frame.add", "params":{"rect":[editor.menu_at.x, editor.menu_at.y, 400, 260]}})));
    rows.push(edit_menu::row("ui.function.annotate", json!({"command":"graph.pin.add", "params":{"at":[editor.menu_at.x, editor.menu_at.y]}})));
    rows.push(edit_menu::row(
        "ui.function.add",
        json!({"operation":"passthrough", "at":[editor.menu_at.x, editor.menu_at.y], "snap":app.ui.prefs.snap_to_grid}),
    ));
    rows.push(edit_menu::row("file.save", json!({})));
    rows.extend(edit_menu::selection_rows(editor.selection_parameters("edit.selectAll")));
    rows.push(edit_menu::row("ui.graph.previousPin", json!({})));
    rows.push(edit_menu::row("ui.graph.nextPin", json!({})));
    if let Some(row) = wire_actions::swap_row(app, SelectionTarget::Function(target)) {
        rows.push(row);
    }
    rows
}
