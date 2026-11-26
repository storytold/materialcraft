use crate::{i18n::tr, menus};
use egui::Ui;

/// The row of one edit command with its parameters.
#[allow(dead_code)] // P3(graph-w12): ge.A11
pub(crate) fn row(command: &str, params: serde_json::Value) -> craft_menu::Node {
    let _ = (command, params);
    todo!("ge.A11")
}

/// The selection edit rows (cut, copy, delete, duplicate) with their parameters.
#[allow(dead_code)] // P3(graph-w12): ge.A11
pub(crate) fn selection_rows(params: serde_json::Value) -> Vec<craft_menu::Node> {
    let _ = params;
    todo!("ge.A11")
}

pub(crate) fn button(ui: &mut Ui, label: &str, command: &str, enabled: bool) -> bool {
    let mut button = egui::Button::new(tr(label));
    if let Some(shortcut) = menus::shortcut_of(command).map(|text| menus::shortcut_label(ui.ctx(), text)) {
        button = button.shortcut_text(shortcut);
    }
    let clicked = ui.add_enabled(enabled, button).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

pub(crate) fn selection(ui: &mut Ui, enabled: impl Fn(&str) -> bool) -> Option<&'static str> {
    let mut chosen = None;
    for (label, command) in [
        ("Paste", "edit.paste"),
        ("Paste without link", "edit.pasteWithoutLinks"),
        ("Select all", "edit.selectAll"),
        ("Copy selection", "edit.copy"),
        ("Cut selection", "edit.cut"),
        ("Delete selection", "edit.delete"),
        ("Delete and relink", "edit.deleteRelink"),
        ("Duplicate selection", "edit.duplicate"),
        ("Duplicate selection without link", "edit.duplicateWithoutLinks"),
        ("Select upstream nodes", "graph.selectUpstream"),
        ("Select downstream nodes", "graph.selectDownstream"),
        ("Disable selection", "graph.node.disable"),
    ] {
        if command == "edit.copy" {
            ui.separator();
        }
        if button(ui, label, command, enabled(command)) {
            chosen = Some(command);
        }
    }
    chosen
}
