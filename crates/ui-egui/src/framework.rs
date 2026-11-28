//! The craft-ui runtime, hosted from MaterialCraft's own eframe `App` (craft-libs docs/craft-framework.md, section
//! 2.2). The desktop app calls [`install`] once, [`MaterialCraftApp::before_input`] in `raw_input_hook`,
//! [`MaterialCraftApp::close_path`] last in `logic`, and [`before_ui`] first in `ui`; the browser build and tests
//! install with [`install_headless`]. The framework dialogs run on [`MaterialCraftApp::framework_dialogs`] after
//! MaterialCraft's own UI.
//!
//! The runtime takes Escape, Tab and the clipboard events for the whole window unless one of MaterialCraft's own egui
//! widgets has the keyboard. The window's Escape is read with [`escape_pressed`] and taken with
//! [`take_window_cancel`]; the graph editor's canvas is declared to the runtime with [`host_canvas`], which keeps the edit
//! chords and the clipboard events in egui's input for MaterialCraft's shortcuts, and Tab while it opens the node search.
//! The focused canvas leaves the keyboard to the shortcuts ([`widget_has_keyboard`]).

use std::sync::{Arc, OnceLock};

use craft_host::target::ShellInfo;
use craft_host::{AppInfo, Host};
use craft_keymap::{Chord, Key, Overrides};
use craft_ui::fonts::{I18nScripts, SystemFonts, UiFonts};
use craft_ui::runtime::commands::{CommandInput, CommandSet as _, NavCommand};
use craft_ui::runtime::env::{RootEnv, Scale};
use craft_ui::runtime::focus::{self, EditSet, FocusTarget, KeyUse};
use craft_ui::runtime::frame::{FrameInput, FrameOutput};
use craft_ui::runtime::keys::{Intent, with_window_intents};
use craft_ui::runtime::overlay::{self, OpenFilter};
use craft_ui::shell::embed::{self, CloseDecision};
use craft_window::policy::Decision;
use craft_window::system::WindowSystem;

use crate::MaterialCraftApp;
use crate::shell::modality::Modality;
use egui::ViewportId;

/// Each pass of the root window, first in `App::ui`: hands the runtime the native window it answers the hit test of
/// (Windows) and fits the window to its monitor once.
#[cfg(not(target_arch = "wasm32"))]
pub use craft_ui::shell::embed::before_ui;

/// The environment-variable prefix of the frame override: `MATERIALCRAFT_TITLEBAR`.
#[cfg(not(target_arch = "wasm32"))]
const FRAME_PREFIX: craft_window::Prefix = craft_window::Prefix::new("MATERIALCRAFT");

/// The scripts of the interface languages, for the font stack.
static SCRIPTS: I18nScripts = I18nScripts;

/// The runtime's first font stack: egui's bundled faces and no installed system fonts. MaterialCraft's own definitions
/// ([`crate::theme::install`]) replace it before anything is drawn.
fn ui_fonts() -> UiFonts<'static> {
    UiFonts::new(&SCRIPTS).system_fonts(SystemFonts::Off)
}

fn install_with(ctx: &egui::Context, shell: ShellInfo, windows: Arc<dyn WindowSystem>, frame: Option<Decision>) -> Result<(), String> {
    let mut spec = craft_ui::InstallSpec::new(shell, windows).app_id(crate::os_window::app_id());
    if let Some(frame) = frame {
        spec = spec.frame(frame);
    }
    embed::install(ctx, spec, &ui_fonts()).map(drop).map_err(|error| format!("the UI runtime could not be installed: {error}"))
}

/// Who draws the main window's frame. The app's bar is the title bar only where the desktop leaves the frame to the
/// app (GNOME on Wayland) or `MATERIALCRAFT_TITLEBAR=app` asks for it; everywhere else the system draws it. Call it
/// once, before the window exists: the desktop app builds its window with [`craft_window::policy::apply`] and hands
/// the decision to [`install`].
#[cfg(not(target_arch = "wasm32"))]
pub fn decide_frame() -> Decision {
    use craft_window::policy::{TitleBarPref, decide_for};
    let pref = if craft_window::Choice::probe(FRAME_PREFIX).frame().decorations() { TitleBarPref::System } else { TitleBarPref::Integrated };
    decide_for(FRAME_PREFIX, craft_host::target::Backend::from_env(), pref)
}

/// Installs the runtime on the desktop window's context, once, before the first pass, with the window's `frame`.
#[cfg(not(target_arch = "wasm32"))]
pub fn install(ctx: &egui::Context, frame: Decision) -> Result<(), String> {
    let shell = ShellInfo::default().backend(craft_host::target::Backend::from_env());
    install_with(ctx, shell, Arc::new(craft_window::system::SystemWindows), Some(frame))
}

/// Installs the runtime on a context with no native window of its own: the browser build's canvas and test windows.
pub fn install_headless(ctx: &egui::Context) {
    if let Err(error) = install_with(ctx, ShellInfo::default(), Arc::new(craft_window::system::NoWindows), None) {
        log::warn!("{error}");
    }
}

/// Who MaterialCraft is to the framework's dialogs (the update dialogs show its name and icon).
pub(crate) fn host() -> Arc<Host> {
    Arc::new(
        Host::builder(AppInfo { id: "materialcraft", name: "MaterialCraft", version: materialcraft_engine::VERSION })
            .icon_png(include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/ai.storyteller.materialcraft.png"))
            .build(),
    )
}

/// Preferences > Shortcuts assigns any chord to a node, so the runtime's F6 and Shift+F6 pane moves are unbound.
/// Overrides are keyed by command id, which the command set gives.
fn command_overrides() -> Arc<Overrides> {
    static OVERRIDES: OnceLock<Arc<Overrides>> = OnceLock::new();
    OVERRIDES
        .get_or_init(|| {
            let mut overrides = Overrides::new();
            for id in [NavCommand::NextPane.id(), NavCommand::PreviousPane.id()] {
                overrides.insert(id, Vec::new());
            }
            Arc::new(overrides)
        })
        .clone()
}

impl MaterialCraftApp {
    /// The framework's part of `App::raw_input_hook`, once per frame before the first pass: takes what the runtime
    /// gathered since the last frame and gives the coming passes the root values (theme mode, interface scale and
    /// direction).
    pub fn before_input(&mut self, ctx: &egui::Context) {
        // MaterialCraft applies the interface scale itself (`step`), so the runtime keeps the zoom it applied last.
        let scale = ctx.zoom_factor();
        embed::before_input(ctx, |output| self.frame_input(ctx, output, scale));
    }

    /// The root values of this frame. MaterialCraft registers no commands with the runtime (it unbinds the framework's
    /// pane moves, [`command_overrides`]) and lets it toggle no chrome, so the output carries nothing it acts on. While one of MaterialCraft's own egui modals shows in the
    /// root window, the runtime blocks the panels floated into windows of their own.
    pub(crate) fn frame_input(&mut self, ctx: &egui::Context, _output: FrameOutput, scale: f32) -> FrameInput {
        use crate::theme::Brightness;
        let mode = match self.ui.prefs.brightness {
            Brightness::Light | Brightness::MediumLight => craft_theme::Mode::Light,
            Brightness::Dark | Brightness::MediumDark => craft_theme::Mode::Dark,
        };
        let direction = if crate::i18n::is_rtl() { craft_i18n::Direction::Rtl } else { craft_i18n::Direction::Ltr };
        let root = RootEnv::new().mode(mode).scale(Scale::new(scale)).direction(direction);
        let app_modal = Modality::capture(self, ctx).is_blocked().then_some(ViewportId::ROOT);
        FrameInput::new(root).commands(CommandInput::new().overrides(command_overrides())).host(Arc::clone(&self.framework_host)).app_modal(app_modal)
    }

    /// The framework's part of `App::logic`, after MaterialCraft's own: the root window's close path. Unsaved work
    /// holds the window open while Save / Discard / Cancel asks ([`crate::unsaved::close_requested`]); a confirmed
    /// quit closes it.
    pub fn close_path(&mut self, ctx: &egui::Context) {
        embed::logic(ctx, || if crate::unsaved::close_requested(self) { CloseDecision::Hold } else { CloseDecision::Close });
    }
}

/// The graph editor's canvas widget, the window's document canvas. The material, function and FX-Map editors share it,
/// so it keeps the focus when one replaces another.
pub fn canvas_id() -> egui::Id {
    egui::Id::unique("materialcraft-graph-canvas")
}

/// Declares the graph editor's canvas, drawn in this pass, as the window's document canvas. With nothing or the canvas
/// focused, the edit chords and the clipboard events stay in egui's input for MaterialCraft's shortcuts. With
/// `opens_search` the focused canvas claims a bare Tab, which opens its node search; Shift+Tab always moves on from
/// it, and Tab does otherwise.
pub(crate) fn host_canvas(ctx: &egui::Context, opens_search: bool) {
    const KEYS: KeyUse = KeyUse::CANVAS.with_edits(EditSet::ALL);
    const TAB: [Chord; 1] = [Chord::plain(Key::Tab)];
    const SEARCH_KEYS: KeyUse = KEYS.with_claims(&TAB);
    // The claim would hold with nothing focused too, where a bare Tab moves the focus into the window
    let claims_tab = opens_search && ctx.memory(|memory| memory.has_focus(canvas_id()));
    focus::host_canvas(ctx, canvas_id(), if claims_tab { SEARCH_KEYS } else { KEYS });
}

/// Gives the canvas the keyboard focus, with the arrows and Escape its own.
pub(crate) fn focus_canvas(ctx: &egui::Context) {
    focus::request(ctx, FocusTarget::Id(canvas_id()));
}

/// The widget that has the keyboard, the canvas left out: the focused canvas leaves the keyboard to MaterialCraft's
/// shortcuts, as with nothing focused.
pub(crate) fn focused_widget(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|memory| memory.focused()).filter(|focused| *focused != canvas_id())
}

/// Whether a widget other than the canvas has the keyboard.
pub(crate) fn widget_has_keyboard(ctx: &egui::Context) -> bool {
    focused_widget(ctx).is_some()
}

/// Whether a dialog, MaterialCraft's, egui's or the framework's, the home screen, or an egui menu or popup is up.
pub(crate) fn overlay_open(app: &MaterialCraftApp, ctx: &egui::Context) -> bool {
    app.ui.windows.home || Modality::capture(app, ctx).is_blocked() || egui::Popup::is_any_open(ctx) || overlay::any_open(ctx, OpenFilter::Modal)
}

/// Whether Escape was pressed in this pass: from egui while one of MaterialCraft's own widgets or popups has the
/// keyboard, otherwise the window's cancel, which the runtime routes to no component. Leaves it for later readers.
pub(crate) fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape)) || window_cancel(ctx, false)
}

/// Takes the window's cancel (an Escape while nothing has framework focus) when it is there: no later reader sees it.
pub(crate) fn take_window_cancel(ctx: &egui::Context) -> bool {
    window_cancel(ctx, true)
}

/// Releases the keyboard from a framework widget (a dock tab row, a panel's drag band) when nothing took the Escape
/// pressed on it, as egui does for MaterialCraft's own widgets. Called last in the UI: what is drawn before it takes its
/// Escape first. A framework modal keeps the focus in it, and a text field its own.
pub(crate) fn release_keyboard(ctx: &egui::Context) {
    if let Some(focused) = focused_widget(ctx)
        && !ctx.text_edit_focused()
        && !overlay::any_open(ctx, OpenFilter::Modal)
        && take_window_cancel(ctx)
    {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
    }
}

/// Whether the window's cancel is there and untaken; `take` takes it.
fn window_cancel(ctx: &egui::Context, take: bool) -> bool {
    let mut pending = false;
    with_window_intents(ctx, false, |intents| {
        pending = if take { intents.take(Intent::Cancel) } else { intents.untaken(&Intent::Cancel) };
        take && pending
    });
    pending
}
