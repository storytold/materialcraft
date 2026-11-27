use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui_kittest::kittest::Queryable as _;

use super::*;
use crate::menus;
use crate::state::UiState;
use materialcraft_engine::sample_session;

const COMMAND: &str = "ui.help.checkForUpdates";

/// A pins file that does not exist: the tests do not read the launcher's real choices
fn no_pins() -> std::path::PathBuf {
    std::env::temp_dir().join("materialcraft-updates-test-missing").join("pins.json")
}

fn app() -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(sample_session(), UiState::default());
    app.updates_rt.updater = Some(Updater::new(&crate::UPDATE_APP, "0.1.0").with_pins_path(no_pins()));

    app
}

/// An app whose checks answer "nothing newer" instead of asking GitHub; the receiver hears each
/// check start, and the counter says how many did
fn app_with_source() -> (MaterialCraftApp, Receiver<()>, Arc<AtomicUsize>) {
    let mut app = app();
    let (tx, rx) = channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let source: craft_update::updater::CheckSource = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
        Ok(None)
    });
    app.updates_rt.updater = Some(Updater::new(&crate::UPDATE_APP, "0.1.0").with_pins_path(no_pins()).with_source(source));
    (app, rx, calls)
}

fn release() -> craft_update::Release {
    craft_update::Release {
        tag: "v99.0.0".into(),
        version: craft_update::parse_version("99.0.0"),
        name: "MaterialCraft 99.0.0".into(),
        html_url: "https://github.com/storytold/materialcraft/releases/tag/v99.0.0".into(),
        ..Default::default()
    }
}

/// Offers `release()` and ticks until the prompt is open.
/// A fresh updater declines an offer only when the pins file holds a launcher choice, which sets
/// `managed()`. A tick asks for another frame while the offer waits for its thread, so the wait
/// fails when a tick leaves the prompt closed and asks for no frame, instead of ticking forever.
fn offer(updater: &mut Updater) {
    updater.offer(release());
    let ctx = egui::Context::default();

    while !updater.prompt_open() {
        assert!(updater.managed().is_none(), "the offer was declined: {} holds a launcher choice", no_pins().display());
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| updater.tick(ctx));
        assert!(updater.prompt_open() || updater.managed().is_some() || ctx.has_requested_repaint(), "the offer ended without opening the prompt");
        std::thread::yield_now();
    }
}

pub(super) fn open_prompt(app: &mut MaterialCraftApp) {
    app.updates_rt.updater = Some(Updater::new(&crate::UPDATE_APP, "0.1.0").with_pins_path(no_pins()));
    let Some(updater) = app.updates_rt.updater.as_mut() else { return };
    offer(updater);
}

#[test]
fn the_update_app_names_this_repository() {
    assert_eq!(crate::UPDATE_APP.id, "materialcraft");
    assert_eq!(crate::UPDATE_APP.name, "MaterialCraft");
    assert_eq!(crate::UPDATE_APP.repo, "storytold/materialcraft");
    assert_eq!(crate::UPDATE_APP.bundle_id(), "ai.storyteller.materialcraft");
    assert!(crate::UPDATE_APP.icon_png.is_some());
}

#[test]
fn the_command_is_in_the_help_menu() {
    let item = menus::CATALOG.iter().find(|item| item.id == COMMAND).unwrap();
    assert_eq!(item.menu, "Help");
    assert_eq!(item.label, "Check for updates...");
    assert!(menus::implemented(COMMAND));
    assert!(menus::enabled(&app(), COMMAND));
}

#[test]
fn a_new_app_is_idle_and_does_not_check_at_start() {
    let rt = Runtime::default();
    assert!(!rt.startup_check);
    assert!(!rt.check_requested);
    let updater = rt.updater.as_ref().expect("a stable build has its own updater");
    assert!(!updater.prompt_open());
    assert_eq!(updater.status(), &Status::Idle);
}

/// One whole desktop-sized frame of the app; the first one only installs the fonts. egui's default screen is 10000
/// points square, where every library tile is in view and the headless app's inline jobs cook every library graph
/// in one pass.
fn frame(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0))), ..Default::default() };
    eframe::App::raw_input_hook(app, ctx, &mut raw);
    let mut output = ctx.run_ui(raw, |ui| {
        app.step(ui.ctx());
        app.draw(ui);
    });
    output.textures_delta.clear();
    eframe::App::logic(app, ctx, &mut eframe::Frame::_new_kittest());
}

#[test]
fn the_menu_command_starts_a_check_on_the_next_frame() {
    let (mut app, checks, _) = app_with_source();
    let ctx = egui::Context::default();
    MaterialCraftApp::install_runtime(&ctx, None).expect("runtime");
    frame(&mut app, &ctx);
    menus::invoke(&mut app, &ctx, COMMAND);
    assert!(app.updates_rt.check_requested);
    assert_eq!(app.updates_rt.updater.as_ref().unwrap().status(), &Status::Idle, "the check waits for the frame");

    frame(&mut app, &ctx);
    assert!(!app.updates_rt.check_requested);
    assert_ne!(app.updates_rt.updater.as_ref().unwrap().status(), &Status::Idle);
    // Dropping the app ends the wait with an error if no check was started
    drop(app);
    checks.recv().unwrap();
}

#[test]
fn the_check_at_start_runs_only_when_turned_on() {
    let (mut app, checks, calls) = app_with_source();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    app.set_startup_update_check(true);
    tick(&mut app, &ctx);
    assert_eq!(app.updates_rt.updater.as_ref().unwrap().status(), &Status::Idle, "the check at start is quiet");
    // Dropping the app ends the wait with an error if no check was started
    drop(app);
    checks.recv().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn result_window_shows_every_status_but_idle() {
    let (mut app, _, _) = app_with_source();
    let ctx = egui::Context::default();
    MaterialCraftApp::install_runtime(&ctx, None).expect("runtime");
    for source_status in ["upToDate", "failed"] {
        app.updates_rt.updater = Some(
            Updater::new(&crate::UPDATE_APP, "0.1.0")
                .with_source(Arc::new(move || if source_status == "failed" { Err("no network".into()) } else { Ok(None) })),
        );
        check(&mut app);
        frame(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
        frame(&mut app, &ctx);
        let host = craft_ui::shell::WindowDialogs::of(&ctx, &craft_ui::runtime::env::WindowKey::main());
        host.with(|host| assert!(host.get::<craft_update::stack_dialogs::UpdateCheckDialog>().is_some()));
    }
}

#[test]
fn an_offered_release_opens_the_prompt_over_the_window() {
    let mut app = app();

    offer(app.updates_rt.updater.as_mut().unwrap());

    let ctx = egui::Context::default();
    MaterialCraftApp::install_runtime(&ctx, None).expect("runtime");
    let host = craft_ui::shell::WindowDialogs::of(&ctx, &craft_ui::runtime::env::WindowKey::main());
    // The prompt waits in the window's startup queue, so the whole window draws.
    for _ in 0..3 {
        frame(&mut app, &ctx);
    }
    host.with(|host| assert!(host.get::<craft_update::stack_dialogs::UpdatePromptDialog>().is_some()));
    assert!(prompt_open(&app));
}

/// The whole window around `app`, with the runtime installed, as the eframe driver runs it.
fn window(app: MaterialCraftApp) -> egui_kittest::Harness<'static, MaterialCraftApp> {
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.run_steps(4);
    harness
}

/// An app whose requested check runs until the returned sender goes away.
fn app_with_a_running_check() -> (MaterialCraftApp, std::sync::mpsc::Sender<()>) {
    let mut app = app();
    let (release, wait) = channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    let source: CheckSource = Arc::new(move || {
        if let Ok(wait) = wait.lock() {
            let _ = wait.recv();
        }
        Ok(None)
    });
    app.updates_rt.updater = Some(Updater::new(&crate::UPDATE_APP, "0.1.0").with_pins_path(no_pins()).with_source(source));
    check(&mut app);
    (app, release)
}

#[test]
fn a_requested_check_shows_its_progress() {
    let (app, release) = app_with_a_running_check();
    let harness = window(app);
    assert_eq!(harness.state().updates_rt.updater.as_ref().unwrap().status(), &Status::Checking);
    assert!(harness.query_by_label_contains("Checking for updates").is_some(), "the result dialog says the check runs");
    drop(release);
}

#[test]
fn the_prompt_and_the_modal_results_hold_the_keyboard() {
    use crate::shell::modality::{Blocker, Modality};

    let idle = window(app());
    assert_eq!(Modality::capture(idle.state(), &idle.ctx), Modality::Free, "an idle updater holds nothing");

    let mut prompted = app();
    offer(prompted.updates_rt.updater.as_mut().unwrap());
    let prompted = window(prompted);
    assert_eq!(Modality::capture(prompted.state(), &prompted.ctx), Modality::Blocked(Blocker::Update));

    let (app, release) = app_with_a_running_check();
    let checking = window(app);
    let host = craft_ui::shell::WindowDialogs::of(&checking.ctx, &craft_ui::runtime::env::WindowKey::main());
    host.with(|host| assert!(host.get::<UpdateCheckDialog>().is_some(), "the result dialog is open"));
    assert_eq!(Modality::capture(checking.state(), &checking.ctx), Modality::Blocked(Blocker::Dialog), "the result dialog is modal");
    drop(release);
}

#[test]
fn shortcuts_wait_while_the_prompt_is_open() {
    for prompt in [false, true] {
        let mut app = app();
        if prompt {
            offer(app.updates_rt.updater.as_mut().unwrap());
        }
        let mut harness = window(app);
        harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::E);
        harness.run_steps(2);
        assert_eq!(harness.state().ui.windows.export, !prompt, "Ctrl+Shift+E opens Export only without the prompt (prompt: {prompt})");
    }
}

#[test]
fn a_nightly_build_has_no_updater_and_points_at_the_launcher() {
    // The Nightly channel never constructs the updater: its installs belong to ArtCraft Launcher.
    let mut app = app();
    app.updates_rt.updater = None;
    assert!(!app.updates_enabled());
    let ctx = egui::Context::default();
    menus::invoke(&mut app, &ctx, COMMAND);
    assert!(!app.updates_rt.check_requested, "no check may start without an updater");
    assert_eq!(app.status, crate::i18n::tr("Updates are managed by ArtCraft Launcher"));
    // A frame without an updater must not panic or touch the pins file.
    tick(&mut app, &ctx);
}
