//! The read-only Shortcuts list.

use craft_menu::shortcuts::{DialogState, FixedRow, Options, ShortcutsDialog};
use egui::Ui;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Shows the app's keymap and its documented gesture shortcuts.
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if !app.ui.windows.shortcuts {
        app.shortcuts = None;
        return;
    }
    let ctx = ui.ctx().clone();
    let Some(keymap) = craft_ui::runtime::commands::keymap(&ctx) else {
        app.shortcuts = None;
        app.ui.windows.shortcuts = false;
        return;
    };
    let state = app.shortcuts.get_or_insert_with(DialogState::new);
    let mut keymap = (*keymap).clone();
    let mut options = Options::default();
    options.title = Some(tr("Shortcuts").to_owned());
    options.editable = false;
    options.summary = false;
    options.fixed = [
            ("Graph", "Node search (over the graph)", "Space / Tab"),
            ("Graph", "Frame all / the selection", "F"),
            ("Graph", "Zoom the graph and the views", "Wheel"),
            ("Graph", "Pan the graph", "Middle drag / Alt+drag"),
            ("Graph", "Show in the 2D view", "Double-click node"),
            ("Graph", "Disable selection", "Shift+D"),
            ("3D View", "Pan the camera", "Middle drag / Alt+drag (3D view)"),
            ("3D View", "Turn the environment", "Shift+drag (3D view)"),
        ]
        .into_iter()
    .map(|(group, label, keys)| FixedRow::new(tr(group), tr(label), keys))
    .collect();
    let _ = ShortcutsDialog::new(&mut keymap).options(&options).show(ui, state);
    if !state.is_open() {
        app.shortcuts = None;
        app.ui.windows.shortcuts = false;
    }
}
