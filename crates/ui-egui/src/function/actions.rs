use super::FunctionEditor;
use crate::MaterialCraftApp;
use craft_ui::runtime::commands::{ClipItem, ClipboardHandle};
use materialcraft_engine::FunctionEdit;
use serde_json::{Value, json};

pub(super) fn editor_enabled(app: &MaterialCraftApp, editor: &FunctionEditor, command: &str) -> Option<bool> {
    if command == "graph.node.edit" {
        return Some(
            editor.selection.len() == 1
                && editor.current(app).zip(editor.selection.first()).and_then(|(graph, id)| graph.node(*id)).is_some_and(|node| node.instance.is_some()),
        );
    }
    let target = editor.command_target().map(materialcraft_engine::SelectionTarget::Function);
    if let Some(enabled) = crate::graph::wire_actions::enabled(app, command, &editor.selected_wire_refs(app), target.as_ref()) {
        return Some(enabled && editor.current(app).is_some());
    }

    if app.session.annotation_selection.is_some() && matches!(command, "edit.delete" | "edit.deleteRelink") {
        return None;
    }
    if let Some(target) = editor.command_target()
        && let Some(enabled) = materialcraft_engine::selection_commands::enabled_for(
            &app.session,
            &materialcraft_engine::SelectionTarget::Function(target),
            &editor.selection,
            command,
        )
    {
        return Some(enabled && editor.current(app).is_some());
    }
    match command {
        "edit.undo" | "edit.redo" | "edit.finish" | "graph.cook" | "ui.graph.snapToGrid" => None,
        "ui.graph.previousPin" | "ui.graph.nextPin" | "graph.comment.add" | "graph.frame.add" | "graph.pin.add" => None,
        command if command.starts_with("graph.") || command.starts_with("ui.graph.") || command.starts_with("edit.") => Some(false),
        _ => None,
    }
}

pub(crate) fn enabled(app: &MaterialCraftApp, command: &str) -> Option<bool> {
    app.function_editor.as_ref().and_then(|editor| editor_enabled(app, editor, command))
}

pub(super) fn invoke_editor(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, command: &str) -> bool {
    let Some(allowed) = editor_enabled(app, editor, command) else { return false };

    if !allowed {
        return true;
    }

    let wires = editor.selected_wire_refs(app);
    if command.starts_with("graph.wire.") || (command == "edit.delete" && !wires.is_empty()) {
        let command = if command == "edit.delete" { "graph.wire.delete" } else { command };
        let target = editor.command_target().map(materialcraft_engine::SelectionTarget::Function);
        // PLAN: graph-w8 owns the retyped edit API; this tree still has the Vec-mutating signature.
        crate::graph::wire_actions::edit(app, &mut wires.clone(), target, command);
        return true;
    }

    if command == "graph.node.edit" {
        editor.open_reference = editor.selection.first().copied();
        return true;
    }
    let parameters = editor.selection_parameters(command);
    crate::graph::selection_commands::invoke(app, &mut editor.selection, command, parameters);
    if matches!(command, "edit.selectAll" | "edit.selectNone" | "edit.paste" | "edit.pasteWithoutLinks" | "edit.duplicate" | "edit.duplicateWithoutLinks") {
        editor.selected_wires = Selection::default();
    }
    true
}

/// The commands `ui.function.annotate` runs: the canvas menu's annotation rows.
const ANNOTATIONS: [&str; 3] = ["graph.comment.add", "graph.frame.add", "graph.pin.add"];

/// Runs the Function canvas menu's own actions on the open editor: `ui.function.copyInfo {id}`,
/// `ui.function.openReference {id}`, `ui.function.setOutput {id}`, `ui.function.add {operation, at, snap}` and
/// `ui.function.annotate {command, params}`. False for any other command or without an editor.
// PLAN: graph-w8's `graph/actions.rs` routing calls this for `ui.function.*` while the Function editor is active (ge.A21).
pub(crate) fn invoke_ui(app: &mut MaterialCraftApp, ctx: &egui::Context, command: &str, params: &Value) -> bool {
    if !command.starts_with("ui.function.") {
        return false;
    }
    let Some(mut editor) = app.function_editor.take() else { return false };

    let id = params.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok());
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
    let handled = match (command, id) {
        ("ui.function.copyInfo", Some(id)) => {
            if let Some(info) = app.run("graph.function.node.info", json!({"target": editor.command_target(), "id": id}))
                && let Some(text) = info.get("text").and_then(Value::as_str)
            {
                // A refused clipboard leaves nothing to undo or report beyond the platform's own notice.
                let _ = ClipboardHandle::of(ctx).offer(&[ClipItem::Text(text.into())]);
            }
            true
        }
        ("ui.function.openReference", Some(id)) => {
            editor.open_reference = Some(id);
            true
        }
        ("ui.function.setOutput", Some(id)) => {
            editor.edit(app, FunctionEdit::Output { id }, false);
            true
        }
        ("ui.function.add", _) => match text("operation") {
            Some(operation) => {
                let at = params.get("at").and_then(|at| serde_json::from_value::<[f32; 2]>(at.clone()).ok()).unwrap_or(editor.menu_at.into());
                let snap = params.get("snap").and_then(Value::as_bool).unwrap_or(app.ui.prefs.snap_to_grid);
                editor.edit(app, FunctionEdit::Add { operation, at, snap }, false);
                true
            }
            None => false,
        },
        ("ui.function.annotate", _) => match text("command").filter(|command| ANNOTATIONS.contains(&command.as_str())) {
            Some(command) => {
                app.run(&command, params.get("params").cloned().unwrap_or_else(|| json!({})));
                editor.selection.clear();
                true
            }
            None => false,
        },
        _ => false,
    };
    finish_editor(app, editor);
    handled
}

pub(crate) fn invoke(app: &mut MaterialCraftApp, command: &str) -> bool {
    let Some(mut editor) = app.function_editor.take() else { return false };

    let handled = invoke_editor(app, &mut editor, command);
    finish_editor(app, editor);
    handled
}

pub(super) fn finish_editor(app: &mut MaterialCraftApp, mut editor: FunctionEditor) {
    let reference =
        editor.open_reference.take().and_then(|id| app.run("graph.instance.reference", serde_json::json!({"id": id, "function": editor.command_target()})));
    app.function_editor = Some(editor);
    if let Some(reference) = reference
        && let Some(graph) = reference.get("graph").and_then(serde_json::Value::as_u64).and_then(|graph| usize::try_from(graph).ok())
    {
        crate::graph::subgraph_navigation::open_external_instance(app, &reference, graph);
    }
}
