//! Software updates on the desktop: the background check at startup, Help > Check for updates and
//! the prompts that report them. A Nightly has no updater: the Launcher installs it.

use craft_update::stack_dialogs::{CheckChoice, UpdateCheckDialog};
use craft_update::updater::{Action, CheckSource, Status, Updater};
use std::sync::Arc;

use crate::MaterialCraftApp;

pub(crate) struct Runtime {
    /// `None` in a Nightly build, whose updates belong to ArtCraft Launcher.
    updater: Option<Updater>,
    /// The desktop app turns the check at start on; tests and scripted runs leave it off
    startup_check: bool,
    check_requested: bool,
    /// The context of the last frame, to wake the UI when a check is queued between frames
    repaint: Option<egui::Context>,
    held_check: Option<std::sync::mpsc::Sender<()>>,
}

impl Default for Runtime {
    fn default() -> Self {
        let updater = crate::os_window::stable_services_for(crate::os_window::identity())
            .then(|| Updater::new(&crate::UPDATE_APP, materialcraft_engine::VERSION).with_translator(translate));
        Self { updater, startup_check: false, check_requested: false, repaint: None, held_check: None }
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

pub(crate) fn tick(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    rt.repaint = Some(ctx.clone());
    let Some(updater) = rt.updater.as_mut() else { return };
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
    let ctx = ui.ctx().clone();
    let host = craft_ui::shell::WindowDialogs::of(&ctx, &craft_ui::runtime::env::WindowKey::main());
    let clean = !crate::unsaved::needs_confirmation(&app.session);
    let action = app.updates_rt.updater.as_mut().map(|updater| host.with(|host| updater.run_with_save(host, clean, false)));
    match action {
        Some(Action::OpenUrl(url)) => {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        Some(Action::Close) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        _ => {}
    }
    let mut check_again = false;
    let Some(updater) = app.updates_rt.updater.as_mut() else { return };
    let pending_prompt = updater.prompt_open();
    host.with(|host| {
        if result_window_shows(updater.status(), pending_prompt) {
            let mut opened = false;
            if host.get::<UpdateCheckDialog>().is_none() {
                host.open(UpdateCheckDialog::new(materialcraft_engine::VERSION));
                opened = true;
            }
            if let Some(dialog) = host.get_mut::<UpdateCheckDialog>() {
                dialog.set_status(updater.status().clone());
                dialog.set_available(updater.available().cloned());
                dialog.set_installing(updater.is_installing());
                dialog.set_message(updater.status_message());
            }
            if !opened && let Some(result) = host.take::<UpdateCheckDialog>() {
                if let Some(choice) = result.ok() {
                    match choice {
                        CheckChoice::Close => updater.dismiss_status(),
                        CheckChoice::CheckAgain | CheckChoice::CheckNow => check_again = true,
                        CheckChoice::RestartToUpdate => updater.restart_to_update(),
                        CheckChoice::Update => {
                            updater.request_install(clean);
                        }
                        CheckChoice::Download => {
                            if let Some(release) = updater.available() {
                                ctx.open_url(egui::OpenUrl::new_tab(release.html_url.clone()));
                            }
                        }
                    }
                }
            }
        } else if host.get::<UpdateCheckDialog>().is_some() {
            host.close_by_id(craft_ui::dialog::dialog_id("updateDialog.check"));
        }
    });
    if check_again {
        app.updates_rt.check_requested = true;
    }
}

/// Installs a fresh updater with a scripted check source and requests a check (`ui.set {updateStatus}`); `available`
/// offers a release, and the browser build answers an error.
pub(crate) fn scripted_status(app: &mut MaterialCraftApp, status: &str, version: Option<&str>, message: Option<&str>) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (app, status, version, message);
        return Err("scripted update status is unavailable in the browser".into());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::mpsc::channel;
        app.updates_rt.held_check = None;
        let version = version.unwrap_or("99.0.0");
        let release = craft_update::Release {
            tag: format!("v{version}"),
            version: craft_update::parse_version(version),
            name: format!("MaterialCraft {version}"),
            html_url: format!("https://github.com/storytold/materialcraft/releases/tag/v{version}"),
            ..Default::default()
        };
        let source: CheckSource = match status {
            "idle" => Arc::new(|| Ok(None)),
            "available" => {
                let r = release.clone();
                Arc::new(move || Ok(Some(r.clone())))
            }
            "upToDate" => Arc::new(|| Ok(None)),
            "failed" => {
                let why = message.unwrap_or("The release list could not be read").to_owned();
                Arc::new(move || Err(why.clone().into()))
            }
            "checking" => {
                let (tx, rx) = channel();
                app.updates_rt.held_check = Some(tx);
                Arc::new(move || {
                    let _ = rx.recv();
                    Ok(None)
                })
            }
            other => return Err(format!("unknown update status `{other}` (idle, checking, upToDate, available, failed)")),
        };
        let mut updater = Updater::new(&crate::UPDATE_APP, materialcraft_engine::VERSION)
            .with_translator(translate)
            .with_pins_path(std::env::temp_dir().join("materialcraft-control-no-pins").join("pins.json"))
            .with_source(source);
        if status == "available" {
            updater.offer(release);
        }
        app.updates_rt.updater = Some(updater);
        app.updates_rt.check_requested = true;
        if let Some(ctx) = &app.updates_rt.repaint {
            ctx.request_repaint();
        }
        Ok(())
    }
}

/// Whether the update prompt shows; it is the updates source of `Modality::capture`.
pub(crate) fn prompt_open(app: &MaterialCraftApp) -> bool {
    app.updates_rt.updater.as_ref().is_some_and(Updater::prompt_open)
}

/// Opens the update prompt for a test in another module
#[cfg(test)]
pub(crate) fn open_prompt_for_test(app: &mut MaterialCraftApp) {
    tests::open_prompt(app);
}

#[cfg(test)]
#[path = "updates_ui.test.rs"]
mod tests;
