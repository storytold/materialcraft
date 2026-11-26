use super::{WireRef, editor_target::EditorTarget};
use crate::{MaterialCraftApp, i18n::tr};
use craft_behavior::Selection;
use craft_keymap::CommandRef;
use craft_menu::{Item, Node};
use craft_ui::{
    contract::{Props as _, ShowContainer as _},
    menu::Menu,
};
use egui::Pos2;
use serde_json::{Value, json};

/// Runs an edit against the selected links and clears their selection after deletion.
pub(crate) fn edit(
    app: &mut MaterialCraftApp,
    refs: &[WireRef],
    selection: &mut Selection<u64>,
    target: Option<materialcraft_engine::SelectionTarget>,
    command: &str,
) -> bool {
    if refs.is_empty() || (command == "graph.wire.swap" && !can_swap(app, refs, target.as_ref())) {
        return false;
    }
    if app.run(command, parameters(app, refs, target.as_ref())).is_some() && command == "graph.wire.delete" {
        *selection = Selection::default();
    }
    true
}

fn parameters(app: &MaterialCraftApp, wires: &[WireRef], target: Option<&materialcraft_engine::SelectionTarget>) -> Value {
    let mut parameters = json!({"graph": app.session.graph, "wires": wires.iter().map(|(to, port)| json!({"to": to.0, "toPort": port})).collect::<Vec<_>>()});
    match target {
        Some(materialcraft_engine::SelectionTarget::Function(target)) => parameters["function"] = json!(target),
        Some(materialcraft_engine::SelectionTarget::Pattern(owner)) => parameters["pattern"] = json!(owner.0),
        None => {}
    }
    parameters
}

fn can_swap(app: &MaterialCraftApp, wires: &[WireRef], target: Option<&materialcraft_engine::SelectionTarget>) -> bool {
    wires.len() == 2 && materialcraft_engine::wire_swap::available(&app.session, &parameters(app, wires, target))
}

/// Reports whether a wire command can run for the current selection.
pub(crate) fn enabled(app: &MaterialCraftApp, command: &str, refs: &[WireRef], target: Option<&materialcraft_engine::SelectionTarget>) -> Option<bool> {
    match command {
        "graph.wire.swap" => Some(can_swap(app, refs, target)),
        "graph.wire.delete" => Some(!refs.is_empty()),
        "edit.delete" if !refs.is_empty() => Some(true),
        _ => None,
    }
}

pub(crate) fn invoke_selected(app: &mut MaterialCraftApp, command: &str) -> bool {
    match EditorTarget::resolve(app) {
        EditorTarget::Function => {
            let Some(mut editor) = app.function_editor.take() else { return false };
            let refs = editor.selected_wire_refs(app);
            let target = editor.command_target().map(materialcraft_engine::SelectionTarget::Function);
            let mut selection = std::mem::take(&mut editor.selected_wires);
            let handled = edit(app, &refs, &mut selection, target, command);
            editor.selected_wires = selection;
            app.function_editor = Some(editor);
            handled
        }
        EditorTarget::Pattern => {
            let Some(mut editor) = app.pattern_editor.take() else { return false };
            let refs = editor.selected_wire_refs(app);
            let target = Some(materialcraft_engine::SelectionTarget::Pattern(editor.node));
            let mut selection = std::mem::take(&mut editor.selected_wires);
            let handled = edit(app, &refs, &mut selection, target, command);
            editor.selected_wires = selection;
            app.pattern_editor = Some(editor);
            handled
        }
        EditorTarget::Material => {
            let refs = app.graph.selected_wire_refs();
            let mut selection = std::mem::take(&mut app.graph.selected_wires);
            let handled = edit(app, &refs, &mut selection, None, command);
            app.graph.selected_wires = selection;
            handled
        }
    }
}

/// Deletes the selected links in the active graph editor.
pub fn delete_selected_wires(app: &mut MaterialCraftApp) -> bool {
    invoke_selected(app, "graph.wire.delete")
}

/// Swaps the sources of the selected links in the active graph editor.
pub fn swap_selected_wires(app: &mut MaterialCraftApp) -> bool {
    invoke_selected(app, "graph.wire.swap")
}

/// Builds the swap command row when exactly two selected links can be swapped.
pub(crate) fn swap_row(app: &MaterialCraftApp, target: materialcraft_engine::SelectionTarget) -> Option<Node> {
    let refs = match &target {
        materialcraft_engine::SelectionTarget::Function(_) => app.function_editor.as_ref()?.selected_wire_refs(app),
        materialcraft_engine::SelectionTarget::Pattern(_) => app.pattern_editor.as_ref()?.selected_wire_refs(app),
    };
    can_swap(app, &refs, Some(&target)).then(|| Item::new(CommandRef::new("graph.wire.swap"), tr("Swap links")).into())
}

/// Shows the context menu for a link, with actions routed through the command board.
pub(super) fn wire_menu(app: &mut MaterialCraftApp, ui: &mut egui::Ui, opener: &egui::Response, wire: WireRef, at: Pos2) {
    let selected = app.graph.selected_wire_refs();
    let count = selected.len();
    let wires = if count < 2 { vec![wire.clone()] } else { selected };
    let delete_label = if count >= 2 { tr("Delete selected links") } else { tr("Delete link") };
    let mut rows = vec![
        Item::new(CommandRef::with_params("graph.wire.insertDot", json!({"to": wire.0.0, "toPort": wire.1, "at": [at.x, at.y]})), tr("Insert reroute dot"))
            .into(),
    ];
    if can_swap(app, &wires, None) {
        rows.push(Item::new(CommandRef::new("graph.wire.swap"), tr("Swap links")).into());
    } else if let Some(row) = match EditorTarget::resolve(app) {
        EditorTarget::Function => app
            .function_editor
            .as_ref()
            .and_then(|editor| editor.command_target())
            .and_then(|target| swap_row(app, materialcraft_engine::SelectionTarget::Function(target))),
        EditorTarget::Pattern => app.pattern_editor.as_ref().and_then(|editor| swap_row(app, materialcraft_engine::SelectionTarget::Pattern(editor.node))),
        EditorTarget::Material => None,
    } {
        rows.push(row);
    }
    rows.push(Item::new(CommandRef::new("graph.wire.delete"), delete_label).into());
    Menu::context(tr("Link"), opener).show(ui, |ui, _| {
        craft_menu::context::rows(ui, &rows);
    });
}
