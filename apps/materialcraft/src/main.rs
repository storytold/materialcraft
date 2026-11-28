//! MaterialCraft desktop app
//! Usage: `materialcraft [package.sbs] [--control <port>] [--empty] [--version]`
//!
//! A Nightly build takes the Launcher's protocol instead:
//! `materialcraft --launch-profile nightly-v1 --profile-root <abs> -- [files…]`, and answers
//! `… --version` before any profile, window or logging work.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(test)]
mod build_channel;
mod build_flavor;
mod control_server;
mod launch_profile;
mod nightly_session;
mod process_identity;
mod profile_root;
mod start_failure;

use materialcraft_engine::Session;
use materialcraft_ui_egui::MaterialCraftApp;
use materialcraft_ui_egui::os_window;

/// The window's size when it opens, in points.
const DEFAULT_SIZE: [f32; 2] = [1600.0, 960.0];
/// The smallest size the window can be resized to, in points.
const MIN_SIZE: [f32; 2] = [960.0, 600.0];

const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.materialcraft.png");
const ICON_NIGHTLY_PNG: &[u8] = include_bytes!("../../../assets/app-icon/materialcraft-nightly.png");

/// The version this build names itself with (the Launcher's feed identity for a Nightly).
const BUILT_VERSION: &str = env!("MATERIALCRAFT_BUILT_VERSION");
/// The commit this build was made from, empty when the build could not tell.
const BUILD_SHA: &str = env!("MATERIALCRAFT_BUILD_SHA");

fn version() -> String {
    let date = env!("MATERIALCRAFT_BUILD_DATE");
    let short = BUILD_SHA.get(..7).unwrap_or(BUILD_SHA);
    // A Nightly answers with its exact feed identity and channel, as the Launcher expects.
    if build_flavor::COMPILED == build_flavor::BuildFlavor::NightlyV1 {
        return format!("MaterialCraft {}", build_flavor::version(build_flavor::COMPILED, BUILT_VERSION));
    }
    match (short.is_empty(), date.is_empty()) {
        (true, true) => format!("MaterialCraft {BUILT_VERSION}"),
        (true, false) => format!("MaterialCraft {BUILT_VERSION} ({date})"),
        _ => format!("MaterialCraft {BUILT_VERSION} ({short} {date})"),
    }
}

struct Args {
    file: Option<String>,
    control: Option<u16>,
}

fn parse_args(argv: &[std::ffi::OsString]) -> Result<Option<Args>, String> {
    let mut args = Args { file: None, control: None };
    let argv: Vec<String> = argv.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let mut it = argv.into_iter();
    let mut files_only = false;
    while let Some(a) = it.next() {
        if files_only {
            // A repeated delimiter is still a separator, never a file named `--`.
            if a == "--" {
                continue;
            }
            if args.file.is_none() {
                args.file = Some(a);
            }
            continue;
        }
        match a.as_str() {
            // Everything after the Launcher's `--` is a file, even when it looks like a flag.
            "--" => files_only = true,
            "--version" | "-V" => {
                println!("{}", version());
                return Ok(None);
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(None);
            }
            "--control" => {
                let p = it.next().ok_or("--control needs a port")?;
                args.control = Some(p.parse().map_err(|_| format!("invalid port `{p}`"))?);
            }
            // Kept for scripts written before empty startup became the default
            "--empty" => {}
            f if !f.starts_with('-') => args.file = Some(f.to_string()),
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    if launch_profile::current().is_nightly() && args.control.is_some() {
        return Err("MaterialCraft Nightly does not open the automation control server".into());
    }
    Ok(Some(args))
}

fn initial_session(args: &Args) -> Session {
    let mut s = materialcraft_engine::new_session();
    if let Some(f) = &args.file {
        match materialcraft_engine::file_cmds::open_file(&mut s, f).map_err(|e| e.to_string()) {
            Ok(()) => {}
            Err(e) => eprintln!("materialcraft: could not open {f}: {e}"),
        }
    }
    s
}

fn icon() -> Option<egui::IconData> {
    let bytes = if launch_profile::current().is_nightly() { ICON_NIGHTLY_PNG } else { ICON_PNG };
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}

/// A scripted run (`--control` or `MATERIALCRAFT_NO_PREFS`) gets no update prompt over the window it drives
fn checks_for_updates_at_start(control: Option<u16>, throwaway_prefs: bool) -> bool {
    control.is_none() && !throwaway_prefs
}

fn initialize_nightly_process_identity() -> Result<(), process_identity::IdentityError> {
    process_identity::initialize(build_flavor::COMPILED == build_flavor::BuildFlavor::NightlyV1, os_window::id_for(os_window::LaunchIdentity::NightlyV1))
}

/// Whether a refusal may also open a native message box. Standard error always carries the reason;
/// this only adds a box where one renders without the app's event loop. Never on macOS: rfd's alert
/// there needs the AppKit run loop this start has not reached, so a refused nightly launch hung in
/// an invisible modal instead of exiting (GridCraft Mac smoke, 2026-11-26).
fn refusal_dialog(flavor: build_flavor::BuildFlavor, desktop_session: bool, macos: bool) -> bool {
    build_flavor::needs_native_error(flavor, desktop_session) && !macos
}

fn report_launch_profile_error(error: &str) {
    eprintln!("MaterialCraft launch profile: {error}");
    let desktop_session = cfg!(windows)
        || cfg!(target_os = "macos")
        || (cfg!(target_os = "linux") && ["DISPLAY", "WAYLAND_DISPLAY"].into_iter().any(|key| std::env::var_os(key).is_some_and(|v| !v.is_empty())));
    if refusal_dialog(build_flavor::COMPILED, desktop_session, cfg!(target_os = "macos")) {
        if let Err(error) = initialize_nightly_process_identity() {
            eprintln!("MaterialCraft launch profile: {error}");
            return;
        }
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        let _ = rfd::MessageDialog::new()
            .set_title("MaterialCraft Nightly")
            .set_description(error)
            .set_level(rfd::MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

/// A Nightly answers a version query before any profile work: the plain `--version`/`-V` that
/// packaging scripts and workflows run, or the Launcher's own
/// `--launch-profile nightly-v1 --profile-root <dir> --version`. Stable keeps its own startup.
fn early_version_query(flavor: build_flavor::BuildFlavor, argv: &[std::ffi::OsString]) -> bool {
    flavor == build_flavor::BuildFlavor::NightlyV1 && (build_flavor::version_query(argv) || launch_profile::version_query(argv))
}

/// The usage text a plain `--help`/`-h` prints, before anything needs a profile.
fn print_usage() {
    println!("{}\n\nUsage: materialcraft [package.sbs] [--control <port>] [--empty]", version());
}

/// A bare help query, which the Linux launchers pass through without a profile, exactly as the
/// version query above is answered: help is not a file name and needs no data folder.
fn early_help_query(argv: &[std::ffi::OsString]) -> bool {
    matches!(argv, [only] if matches!(only.to_str(), Some("--help" | "-h")))
}

/// Select the immutable launch profile before any startup service or side effect.
fn prepare_launch() -> Option<Vec<std::ffi::OsString>> {
    // The trusted Launcher protocol is decided before logging, updates, fonts or saved state.
    let argv: Vec<_> = std::env::args_os().skip(1).collect();
    if early_version_query(build_flavor::COMPILED, &argv) {
        println!("{}", version());
        return None;
    }
    if early_help_query(&argv) {
        print_usage();
        return None;
    }
    let early = launch_profile::parse(argv)
        .and_then(|launch| {
            build_flavor::require_profile(build_flavor::COMPILED, &launch.profile)?;
            initialize_nightly_process_identity().map_err(|error| error.to_string())?;
            Ok(launch)
        })
        .and_then(|launch| {
            launch_profile::prepare(launch, |root| launch_profile::validate_root(root, |key| std::env::var_os(key), std::env::current_exe().ok().as_deref()))
        })
        .unwrap_or_else(|error| {
            report_launch_profile_error(&error);
            std::process::exit(2)
        });
    let identity = if early.profile.is_nightly() { os_window::LaunchIdentity::NightlyV1 } else { os_window::LaunchIdentity::Stable };
    if let Err(error) = os_window::select_identity(identity).and_then(|()| match early.profile.root() {
        Some(root) => materialcraft_ui_egui::data_root::install(root.to_path_buf()),
        None => Ok(()),
    }) {
        report_launch_profile_error(&error);
        std::process::exit(2);
    }
    if let Err(error) = launch_profile::install(early.profile) {
        report_launch_profile_error(&error);
        std::process::exit(2);
    }

    Some(early.args)
}

/// Where this launch keeps its window layout and settings: the private profile for a Nightly,
/// eframe's own folder (or the scripted throwaway file) for stable.
fn persistence_path() -> Option<std::path::PathBuf> {
    if launch_profile::current().is_nightly() {
        return materialcraft_ui_egui::data_root::sub("app.ron");
    }
    std::env::var_os("MATERIALCRAFT_NO_PREFS")
        .map(|_| std::env::temp_dir().join(format!("materialcraft-noprefs-{}.ron", std::process::id())))
        .or_else(|| std::env::var_os("MATERIALCRAFT_DATA_DIR").filter(|v| !v.is_empty()).map(|dir| std::path::PathBuf::from(dir).join("app.ron")))
}

fn main() {
    let Some(arguments) = prepare_launch() else { return };
    let nightly = launch_profile::current().is_nightly();

    // Windows: the update helper is a copy of this program started with `--finish-update`.
    #[cfg(windows)]
    if !nightly && let Some(result) = craft_update::self_update::finish(&materialcraft_ui_egui::UPDATE_APP, &std::env::args_os().skip(1).collect::<Vec<_>>()) {
        std::process::exit(i32::from(result.is_err()));
    }
    // The Launcher protocol's own tokens are consumed here, so parse what it left.
    let args = match parse_args(&arguments) {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            eprintln!("materialcraft: {e}");
            std::process::exit(2);
        }
    };

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("materialcraft: internal error: {info}");
        prev(info);
    }));
    let session = initial_session(&args);
    let window_frame = materialcraft_ui_egui::framework::decide_frame();
    let mut viewport = craft_window::policy::apply(
        egui::ViewportBuilder::default().with_title(os_window::name_for(os_window::identity())).with_inner_size(DEFAULT_SIZE).with_min_inner_size(MIN_SIZE),
        &window_frame.frame,
    );
    // Stable keeps eframe's implicit app id, so its saved layout stays where it is; a Nightly
    // carries its own id so docks, compositor rules and the taskbar list it separately.
    if nightly {
        viewport = viewport.with_app_id(os_window::app_id());
    }
    if let Some(i) = icon() {
        viewport = viewport.with_icon(i);
    }
    // Scripted runs (agents, tests) set MATERIALCRAFT_NO_PREFS so they neither read nor overwrite the user's saved layout and settings: state goes to a throwaway file
    let persistence_path = persistence_path();
    let control = args.control;
    // Throwaway preferences are MATERIALCRAFT_NO_PREFS, not "a persistence path exists": a stable
    // run that only redirects MATERIALCRAFT_DATA_DIR keeps its storage and its update prompt.
    let throwaway_prefs = std::env::var_os("MATERIALCRAFT_NO_PREFS").is_some();
    let startup_update_check = !nightly && checks_for_updates_at_start(control, throwaway_prefs);
    let options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Wgpu, persistence_path, ..Default::default() };

    // After the primary claim, before entering the window loop. No document names, paths or
    // diagnostic payload is collected.
    let (nightly_session, session_health_warning) =
        match nightly_session::Session::begin_with_health(launch_profile::current().root(), BUILT_VERSION, (!BUILD_SHA.is_empty()).then_some(BUILD_SHA)) {
            Ok(state) => state,
            Err(error) => {
                log::warn!("Nightly session health is unavailable: {error}");
                (None, true)
            }
        };
    let previous_nightly_unclean = nightly_session.as_ref().is_some_and(nightly_session::Session::previous_unclean);
    let shared_session: nightly_session::Shared = std::sync::Arc::new(std::sync::Mutex::new(nightly_session));
    // Set once a window has posted the interruption notice; until then the record must survive.
    let unclean_notice_shown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let exit_session = shared_session.clone();
    let exit_notice = unclean_notice_shown.clone();

    let r = eframe::run_native(
        os_window::name_for(os_window::identity()),
        options,
        Box::new(move |cc| {
            materialcraft_ui_egui::framework::install(&cc.egui_ctx, window_frame)?;
            let mut app = MaterialCraftApp::new(cc, session);
            app.build_version = BUILT_VERSION.to_owned();
            app.build_commit = BUILD_SHA.to_owned();
            // macOS quits inside AppKit's terminate, so the record is finished from the exit hook.
            let hook_session = exit_session.clone();
            let hook_notice = exit_notice.clone();
            app.at_exit = Some(Box::new(move || {
                nightly_session::finish_once(&hook_session, hook_notice.load(std::sync::atomic::Ordering::Relaxed));
            }));
            // Called from the creator: winit installs its default menu before this runs. On error winit's menu stays, whose Quit does not ask
            #[cfg(target_os = "macos")]
            if let Err(error) = craft_menu::mac::install_app_menu(&cc.egui_ctx, os_window::name_for(os_window::identity())) {
                log::warn!("could not install the app menu: {error}");
            }
            app.set_startup_update_check(startup_update_check);
            if let Some(port) = control {
                app.control = Some(control_server::start(port, cc.egui_ctx.clone()));
            }
            if previous_nightly_unclean || session_health_warning {
                let message = if previous_nightly_unclean {
                    "The previous MaterialCraft Nightly session did not finish normally. You can report a problem from About. No diagnostics have been sent."
                } else {
                    "MaterialCraft Nightly could not record session health in its private profile. You can still edit, but abnormal-exit detection is unavailable."
                };
                app.toast(message, false);
                exit_notice.store(previous_nightly_unclean, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(Box::new(app))
        }),
    );
    // This is a controlled return: a handled startup error is not an abnormal exit. The exit hook
    // normally already finished the record; this covers a start that created no window.
    nightly_session::finish_once(&shared_session, unclean_notice_shown.load(std::sync::atomic::Ordering::Relaxed));
    if let Err(e) = r {
        eprintln!("materialcraft: {e}");
        // Say why instead of exiting silently (`start_failure`).
        start_failure::report(&e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A caller-written `--` separates files; it never becomes one, however many arrive.
    #[test]
    fn the_file_delimiter_is_never_opened_as_a_path() {
        let args = |values: &[&str]| values.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        let parsed = parse_args(&args(&["--", "--version"])).expect("a file after the delimiter").expect("startup");
        assert_eq!(parsed.file.as_deref(), Some("--version"));
        assert_eq!(parsed.control, None);
        let parsed = parse_args(&args(&["--", "--", "graph.sbs"])).expect("one path").expect("startup");
        assert_eq!(parsed.file.as_deref(), Some("graph.sbs"));
        let parsed = parse_args(&args(&["a.sbs", "--", "b.sbs"])).expect("the first path wins").expect("startup");
        assert_eq!(parsed.file.as_deref(), Some("a.sbs"));
        assert!(parse_args(&args(&["--control", "nonsense"])).is_err());
        assert!(parse_args(&args(&["--unknown"])).is_err());
    }

    /// The one line every packaging script and workflow compares, in both accepted forms.
    #[test]
    fn a_nightly_answers_plain_and_launcher_version_queries() {
        let args = |values: &[&str]| values.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        let nightly = build_flavor::BuildFlavor::NightlyV1;
        assert!(early_version_query(nightly, &args(&["--version"])));
        assert!(early_version_query(nightly, &args(&["-V"])));
        // A profile root is an absolute path on its own platform; `/...` is not absolute on Windows.
        let root = if cfg!(windows) { r"C:\nightly-data\profiles\materialcraft" } else { "/nightly-data/profiles/materialcraft" };
        let launcher = args(&["--launch-profile", "nightly-v1", "--profile-root", root]);
        for flag in ["--version", "-V"] {
            let mut argv = launcher.clone();
            argv.push(flag.into());
            assert!(early_version_query(nightly, &argv), "{flag}");
        }
        // Everything else keeps the ordinary startup: another build channel, a file named
        // `--version`, an incomplete protocol, and the option that only looks like a query.
        assert!(!early_version_query(build_flavor::BuildFlavor::Stable, &args(&["--version"])));
        assert!(!early_version_query(nightly, &args(&["--", "--version"])));
        assert!(!early_version_query(nightly, &args(&["--version", "graph.sbs"])));
        assert!(!early_version_query(nightly, &args(&["--launch-profile", "nightly-v1", "--version"])));
        assert!(!early_version_query(nightly, &[]));
    }

    #[test]
    fn only_a_run_a_person_started_checks_for_updates_at_start() {
        assert!(checks_for_updates_at_start(None, false));
        assert!(!checks_for_updates_at_start(Some(9000), false));
        assert!(!checks_for_updates_at_start(None, true));
    }

    #[test]
    fn startup_without_a_file_has_one_empty_graph() {
        let s = initial_session(&Args { file: None, control: None });
        assert_eq!(s.pkg.name, "Untitled package");
        assert_eq!(s.pkg.graphs.len(), 1);
        let g = s.graph().unwrap();
        assert_eq!(g.name, "New_graph");
        assert!(g.nodes.is_empty());
        assert!(g.connections.is_empty());
        assert!(s.path.is_none());
        assert!(!s.modified);
        assert!(s.history().is_empty());
    }

    #[test]
    fn startup_opens_the_requested_package() {
        let directory = std::env::temp_dir();
        let filename = format!("mc-startup-{}.sbs", std::process::id());
        let path = directory.join(&filename);
        let original = materialcraft_engine::sample_session();
        let bytes = materialcraft_engine::file_cmds::save_bytes(&original).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let canonical = path.canonicalize().unwrap();

        // Startup stores the canonical path, including expanded short names on Windows
        for requested in [path.clone(), directory.join(".").join(&filename)] {
            let session = initial_session(&Args { file: Some(requested.to_string_lossy().into_owned()), control: None });
            assert_eq!(session.pkg.graphs.len(), original.pkg.graphs.len());
            assert_eq!(session.pkg.graphs[0].name, original.pkg.graphs[0].name);
            assert_eq!(materialcraft_engine::file_cmds::save_bytes(&session).unwrap(), bytes);
            assert_eq!(session.path.as_deref(), canonical.to_str());
            assert!(!session.modified);
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod refusal_dialog_tests {
    use super::refusal_dialog;
    use crate::build_flavor::BuildFlavor;

    #[test]
    fn a_refusal_never_blocks_on_a_modal_before_the_event_loop_on_macos() {
        assert!(refusal_dialog(BuildFlavor::NightlyV1, true, false), "a desktop refusal still shows the box where one renders");
        assert!(!refusal_dialog(BuildFlavor::NightlyV1, true, true), "macOS has no run loop yet: the modal would hang");
        assert!(!refusal_dialog(BuildFlavor::NightlyV1, false, false), "a headless session has no box");
        assert!(!refusal_dialog(BuildFlavor::Stable, true, false), "only a nightly refusal explains itself in a box");
    }
}
