//! The Shortcuts list window.

use egui::Ui;

use crate::MaterialCraftApp;

/// The Shortcuts list: a read-only shortcuts dialog over the runtime's keymap, dispatched by `windows::show`.
#[allow(dead_code)] // P3(shell-w14): sh.A25
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let _ = (app, ui);
    todo!("sh.A25")
}
