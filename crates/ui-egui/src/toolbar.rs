//! The main toolbar under the menu bar and the status bar at the bottom of the window.

use craft_theme::variant::Tone;
use craft_ui::bars::{Bar, BarItem, BarItems, BarKind, StatusMessage};
use craft_ui::contract::{BodyCx, Show as _, ShowContainer as _};
use craft_ui::display::{Shade, Spinner, Text};
use craft_ui::style::TypeRole;
use egui::Ui;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// The toolbar's commands in their groups: each group is one strip, and the bar's gap parts the strips.
const GROUPS: [&[&str]; 3] = [&["ui.newGraph", "file.open", "file.save"], &["edit.undo", "edit.redo"], &["ui.export"]];

/// The body of the shell's options bar slot: the file, history and export commands, with the product name at the end.
/// Each button's icon, tooltip and enabled state come from its command, so a dialog that blocks a command disables
/// its button.
pub fn main_toolbar(_app: &mut MaterialCraftApp, ui: &mut Ui, _cx: &mut BodyCx<'_>) {
    Bar::new(tr("Main toolbar"), BarKind::Tool)
        .trailing(|ui, _| {
            Text::new("MaterialCraft").role(TypeRole::BodyStrong).tone(Tone::Accent).show(ui);
        })
        .show(ui, |ui, _| {
            for group in GROUPS {
                BarItems::new(group.iter().map(|&command| BarItem::new(command)).collect()).show(ui);
            }
        });
}

/// The body of the shell's status slot: the engine state or the last status message, and the engine readout at the
/// end.
pub fn status_bar(app: &mut MaterialCraftApp, ui: &mut Ui, _cx: &mut BodyCx<'_>) {
    let busy = !app.cooker.suspended && app.cooker.busy();
    let message = if app.cooker.suspended {
        StatusMessage::new(tr("Engine suspended: choose Suspend engine in the Tools menu again (Shift+Escape) to resume")).tone(Tone::Warn)
    } else if busy {
        StatusMessage::new(tr("Computing..."))
    } else {
        StatusMessage::new(app.status.clone())
    };
    let readout = readout(app);
    Bar::new(tr("Status bar"), BarKind::Status)
        .trailing(move |ui, _| {
            Text::new(readout.clone()).shade(Shade::Faint).show(ui);
        })
        .show(ui, |ui, _| {
            if busy {
                Spinner::new().show(ui);
            }
            message.show(ui);
        });
}

/// Engine threads, last compute time, preview size, cache size and version, in one line.
fn readout(app: &MaterialCraftApp) -> String {
    let cache = app.session.cache.lock().map(|c| c.bytes()).unwrap_or(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let preview = app.session.preview_size.map_or_else(|| tr("full size").to_string(), |p| format!("{p} px"));
    format!(
        "{}: CPU ({threads} {})  |  {}: {:.0} ms  |  {}: {preview}  |  {}: {} MB  |  {} {}",
        tr("Engine"),
        tr("threads"),
        tr("Last compute"),
        app.session.cooked.ms,
        tr("Preview"),
        tr("Cache"),
        cache >> 20,
        tr("Version"),
        materialcraft_engine::VERSION,
    )
}

#[cfg(test)]
mod tests {
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{By, NodeT, Queryable};

    use crate::{MaterialCraftApp, state::UiState};

    fn harness(block: impl FnOnce(&mut MaterialCraftApp)) -> Harness<'static, MaterialCraftApp> {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
        app.ui.windows.home = false;
        block(&mut app);
        let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |cc| {
            MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
            app
        });
        harness.run_steps(3);
        harness
    }

    /// Whether the toolbar's button for `command`, found by the command's label, is disabled.
    fn off(harness: &Harness<'static, MaterialCraftApp>, command: &str) -> bool {
        let label = craft_ui::runtime::commands::resolve(&harness.ctx, command).label.clone();
        let bar = harness.get(By::new().role(Role::Toolbar).label(super::tr("Main toolbar")));
        bar.get(By::new().role(Role::Button).label(&label)).accesskit_node().is_disabled()
    }

    #[test]
    fn toolbar_buttons_match_the_commands_a_blocker_allows() {
        let all = ["ui.newGraph", "file.open", "file.save", "ui.export"];
        let free = harness(|_| {});
        assert!(all.iter().all(|command| !off(&free, command)), "nothing blocks: every button is enabled");

        let dialog = harness(|app| app.ui.windows.about = true);
        assert!(all.iter().all(|command| off(&dialog, command)), "About is open: every button is disabled");

        let file_dialog = harness(|app| {
            crate::native_dialogs::ask(app, crate::windows::package_dialog(true, false), crate::native_dialogs::Then::OpenPackage);
        });
        assert!(!off(&file_dialog, "file.open"), "a pending pick leaves Open enabled: a second request does nothing");
        assert!(off(&file_dialog, "file.save"));
    }
}
