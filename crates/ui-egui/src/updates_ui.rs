//! Software updates on the desktop: the background check at startup, Help > Check for updates and
//! the prompts that report them. A Nightly has no updater: the Launcher installs it.

use craft_update::updater::{Action, Status, Updater};

use crate::MaterialCraftApp;

pub(crate) struct Runtime {
    /// `None` in a Nightly build, whose updates belong to ArtCraft Launcher.
    updater: Option<Updater>,
    /// The desktop app turns the check at start on; tests and scripted runs leave it off
    startup_check: bool,
    check_requested: bool,
    /// The context of the last frame, to wake the UI when a check is queued between frames
    repaint: Option<egui::Context>,
}

impl Default for Runtime {
    fn default() -> Self {
        let updater = crate::os_window::stable_services_for(crate::os_window::identity())
            .then(|| Updater::new(&crate::UPDATE_APP, materialcraft_engine::VERSION).with_translator(translate));
        Self { updater, startup_check: false, check_requested: false, repaint: None }
    }
}

/// The updater's words in the interface language (English where the catalog has no row)
pub fn translate(text: &str, args: &[(&str, &str)]) -> String {
    craft_update::translate::fill(crate::i18n::tr(text), args)
}

impl MaterialCraftApp {
    /// Whether the app asks for a newer release when it starts (off until the desktop app turns it on)
    pub fn set_startup_update_check(&mut self, on: bool) {
        self.updates_rt.startup_check = on;
    }

    /// Whether this build installs its own updates (a Nightly is managed by ArtCraft Launcher).
    pub fn updates_enabled(&self) -> bool {
        self.updates_rt.updater.is_some()
    }
}

/// Help > Check for updates: the next frame starts a check whose outcome the result dialog shows
pub(crate) fn check(app: &mut MaterialCraftApp) {
    if app.updates_rt.updater.is_none() {
        app.toast(crate::i18n::tr("Updates are managed by ArtCraft Launcher"), false);
        return;
    }
    app.updates_rt.check_requested = true;
    if let Some(ctx) = &app.updates_rt.repaint {
        ctx.request_repaint();
    }
}

/// The update prompt and the modal result dialog own the keyboard while they are open: shortcuts
/// must not edit the graph behind them
pub(crate) fn blocks_input(app: &MaterialCraftApp) -> bool {
    let Some(updater) = &app.updates_rt.updater else { return false };
    holds_keyboard(updater.status(), updater.prompt_open())
}

/// The notice that an update installed leaves the app usable behind it
fn holds_keyboard(status: &Status, prompt_open: bool) -> bool {
    prompt_open || (result_window_shows(status, false) && !matches!(status, Status::Updated(_)))
}

pub(crate) fn tick(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    rt.repaint = Some(ctx.clone());
    let Some(updater) = rt.updater.as_mut() else { return };
    // P3(shell-w14): removed upstream: `check_now` and `check_at_start` no longer take the context
    if std::mem::take(&mut rt.check_requested) {
        updater.check_now();
    } else if rt.startup_check {
        updater.check_at_start();
    }
    updater.tick(ctx);
}

/// Whether the result dialog shows: for every status but idle (a requested check, and the first
/// start after an update, which reports that it installed or why it failed), and never over the
/// startup prompt
fn result_window_shows(status: &Status, prompt_open: bool) -> bool {
    !prompt_open && !matches!(status, Status::Idle)
}

/// The software-update prompt, dispatched by `windows::show`.
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    // P3(shell-w14): sh.A24 (removed upstream: `Updater::show_if` and `CheckResultDialog`): the prompt is
    // drawn through `run_with_save` and `UpdateCheckDialog` on the main window's dialog host.
    let _ = (app, ui, Action::None);
}

/// Installs a fresh updater with a scripted check source and requests a check (`ui.set {updateStatus}`); `available`
/// offers a release, and the browser build answers an error.
#[allow(dead_code)] // P3(shell-w14): sh.A19
pub(crate) fn scripted_status(app: &mut MaterialCraftApp, status: &str, version: Option<&str>, message: Option<&str>) -> Result<(), String> {
    let _ = (app, status, version, message);
    todo!("sh.A19")
}

/// Whether the update prompt shows; it is the updates source of `Modality::capture`.
#[allow(dead_code)] // P3(shell-w14): sh.A24
pub(crate) fn prompt_open(app: &MaterialCraftApp) -> bool {
    let _ = app;
    todo!("sh.A24")
}

/// Opens the update prompt for a test in another module
#[cfg(test)]
pub(crate) fn open_prompt_for_test(app: &mut MaterialCraftApp) {
    tests::open_prompt(app);
}

#[cfg(test)]
#[path = "updates_ui.test.rs"]
mod tests;
