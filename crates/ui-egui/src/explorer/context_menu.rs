use super::{ExplorerTarget, explorer_menu_state::ExplorerMenuState};
use crate::menus::{self, Row};
use crate::{i18n::tr, menus::shortcut_label};

/// Rows the browser can't offer: it has no folders to show or package files to open
fn hidden(command: &str) -> bool {
    cfg!(target_arch = "wasm32") && matches!(command, menus::EXPLORER_SHOW_IN_FOLDER | menus::EXPLORER_OPEN_DEPENDENCIES)
}

/// Draw one action with its shortcut and disabled-state explanation.
fn item(ui: &mut egui::Ui, label: &str, command: &'static str, shortcut: Option<&str>, enabled: bool, action: &mut Option<&'static str>) {
    let mut button = egui::Button::new(tr(label));
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(shortcut_label(ui.ctx(), shortcut));
    }
    let response = ui.add_enabled(enabled, button);
    let response = if command.is_empty() { response.on_disabled_hover_text(tr("Not available yet")) } else { response };
    if response.clicked() {
        *action = Some(command);
        ui.close();
    }
}

/// Draw menu rows while respecting resource-folder and platform restrictions.
/// Consecutive rows that share a submenu draw as that submenu.
fn rows(ui: &mut egui::Ui, rows: &[Row], state: &ExplorerMenuState, resources: bool, action: &mut Option<&'static str>) {
    let mut at = 0;
    while at < rows.len() {
        let row = rows[at];
        let Some(label) = row.sub else {
            if row.label == "-" {
                ui.separator();
            } else if !hidden(row.id) {
                item(ui, row.label, row.id, row.shortcut, state.enabled(row.id), action);
            }
            at += 1;
            continue;
        };
        let end = rows[at..].iter().position(|entry| entry.sub != Some(label)).map_or(rows.len(), |len| at + len);
        let entries = &rows[at..end];
        at = end;
        let available = entries.iter().any(|entry| !entry.id.is_empty());
        ui.add_enabled_ui(available, |ui| {
            let submenu = ui.menu_button(tr(label), |ui| {
                for entry in entries {
                    // Resource folders hold folders and resources, not graphs
                    if resources && label == "New" && entry.id != "folder.create" {
                        continue;
                    }

                    item(ui, entry.label, entry.id, None, state.enabled(entry.id), action);
                }
            });
            submenu.response.on_disabled_hover_text(tr("Not available yet"));
        });
    }
}

/// A row of a resource folder's own menu.
const fn folder_row(label: &'static str, id: &'static str, shortcut: &'static str) -> Row {
    Row { menu: "explorer.folder", sub: None, label, id, shortcut: Some(shortcut) }
}

/// Build the context menu for a package, folder, graph or resource.
pub(super) fn show(ui: &mut egui::Ui, opener: &egui::Response, target: &ExplorerTarget, state: &ExplorerMenuState) -> Option<&'static str> {
    let _ = opener; // P3(panels-w5): pn.A34
    ui.set_min_width(220.0);
    let mut action = None;
    let resources = target.is_resources();
    match target {
        ExplorerTarget::Package { .. } => rows(ui, menus::EXPLORER_PACKAGE, state, false, &mut action),
        ExplorerTarget::Folder { .. } if resources => {
            rows(ui, menus::EXPLORER_NEW, state, true, &mut action);
            rows(ui, menus::EXPLORER_IMPORT, state, true, &mut action);
            if !target.is_resources_root() {
                ui.separator();
                rows(ui, &[folder_row("Rename", "folder.rename", "F2"), folder_row("Remove", "folder.remove", "Delete")], state, true, &mut action);
            }
        }
        ExplorerTarget::Folder { .. } => {
            rows(ui, menus::EXPLORER_NEW, state, false, &mut action);
            ui.separator();
            rows(ui, menus::EXPLORER_FOLDER, state, false, &mut action);
        }
        ExplorerTarget::Graph { .. } => rows(ui, menus::EXPLORER_GRAPH, state, false, &mut action),
        ExplorerTarget::Resource { .. } => {
            for row in menus::EXPLORER_RESOURCE {
                item(ui, row.label, row.id, None, true, &mut action);
            }
        }
        ExplorerTarget::Node { .. } => return None,
    }
    action
}

/// True when the target's row menu offers the command, built from the same `EXPLORER_*` rows as `show`.
#[allow(dead_code)] // P3(panels-w5): pn.A41
pub(super) fn offers(target: &ExplorerTarget, command: &str) -> bool {
    let _ = (target, command);
    todo!("pn.A41")
}
