//! Whether input is blocked: the shortcuts, the canvas keys, the floating panels, file drops and
//! `ui.menu.invoke` all ask [`Modality::capture`]
//!
//! Home leaves the app usable behind it, so it does not block; the Shortcuts list is a modal dialog on the window's
//! dialog host and blocks through it

use craft_ui::runtime::env::WindowKey;
use craft_ui::shell::WindowDialogs;

use crate::MaterialCraftApp;

/// The commands that show a file dialog: behind an open one they bring it forward
#[cfg(not(target_arch = "wasm32"))]
const SHOW_A_FILE_DIALOG: &[&str] = &["file.open", "file.saveAs", "file.saveCopyAs", "file.importBitmap", "file.importVector"];

/// What is holding input back
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blocker {
    /// The unsaved-changes prompt, or the Save dialog it opened
    UnsavedPrompt,
    /// A native file dialog on screen or queued
    #[cfg(not(target_arch = "wasm32"))]
    FileDialog,
    /// The update prompt or the result window of a check
    #[cfg(not(target_arch = "wasm32"))]
    Update,
    /// One of the app's own dialogs, or a modal on the window's dialog host
    Dialog,
}

impl Blocker {
    /// The English name the control channel reports
    pub(crate) fn name(self) -> &'static str {
        match self {
            Blocker::UnsavedPrompt => "the unsaved-changes prompt",
            #[cfg(not(target_arch = "wasm32"))]
            Blocker::FileDialog => "a file dialog",
            #[cfg(not(target_arch = "wasm32"))]
            Blocker::Update => "an update window",
            Blocker::Dialog => "a dialog",
        }
    }

    /// The menu commands that still run behind this blocker
    fn allowed_commands(self) -> &'static [&'static str] {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Blocker::FileDialog => SHOW_A_FILE_DIALOG,
            _ => &[],
        }
    }
}

/// The answer for one moment
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Modality {
    Free,
    Blocked(Blocker),
}

impl Modality {
    /// Reads the app's own state and the main window's dialog host, both current, so a dialog opened this pass
    /// blocks this pass. A file dialog is reported only when nothing else blocks input. On a context with no runtime
    /// the host is created empty on first use (`WindowDialogs::of`), so that source reads free.
    pub(crate) fn capture(app: &MaterialCraftApp, ctx: &egui::Context) -> Modality {
        match open_blocker(app) {
            Some(blocker) => Modality::Blocked(blocker),
            None if WindowDialogs::of(ctx, &WindowKey::main()).with(|host| host.shows_modal()) => Modality::Blocked(Blocker::Dialog),
            #[cfg(not(target_arch = "wasm32"))]
            None if crate::native_dialogs::pending(app) => Modality::Blocked(Blocker::FileDialog),
            None => Modality::Free,
        }
    }

    pub(crate) fn is_blocked(self) -> bool {
        self != Modality::Free
    }

    /// Whether the menu command `id` can run now
    pub(crate) fn allows(self, id: &str) -> bool {
        match self {
            Modality::Free => true,
            Modality::Blocked(blocker) => blocker.allowed_commands().contains(&id),
        }
    }
}

fn open_blocker(app: &MaterialCraftApp) -> Option<Blocker> {
    let windows = &app.ui.windows;
    // The pending prompt also holds an action waiting for its Save dialog
    if app.unsaved.is_some() {
        return Some(Blocker::UnsavedPrompt);
    }

    #[cfg(not(target_arch = "wasm32"))]
    if crate::updates_ui::prompt_open(app) {
        return Some(Blocker::Update);
    }

    #[cfg(any(test, target_arch = "wasm32"))]
    if windows.package_save.is_some() {
        return Some(Blocker::Dialog);
    }

    (app.expose_parameter_dialog.is_some()
        || app.shortcuts.is_some()
        || windows.new_graph
        || windows.export
        || windows.preferences
        || windows.about
        || windows.plugin_manager)
    .then_some(Blocker::Dialog)
}

#[cfg(test)]
#[path = "modality.test.rs"]
mod tests;
