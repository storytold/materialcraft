//! Ask before unsaved work is thrown away
//!
//! Closing a package, closing all packages, reverting or quitting asks Save / Discard / Cancel for each modified
//! package the action touches, one at a time, with the package the question names made active
//! Opening or creating another package retains the existing documents without asking
//! Engine commands run by the control channel, MCP, the CLI and scripts never open a dialog
//!
//! The order of the questions and what each answer leads to is the framework's [`CloseGuard`]; this module holds
//! the guard with the [`Guarded`] action at `app.unsaved`, carries out its steps on the session and runs the action
//! once the guard finishes

use craft_quit::{CloseGuard, CloseScope, GuardStep};
use craft_ui::contract::Bind;
use craft_ui::dialog::{UnsavedAnswer, UnsavedChangesDialog};
use craft_ui::shell::embed::CloseDecision;
use serde_json::{Value, json};

use crate::MaterialCraftApp;
use crate::i18n::tr;

mod guarded;
pub use guarded::Guarded;

/// The held action and where its prompt stands, kept at `app.unsaved` while the unsaved-changes prompt is open.
pub(crate) struct Pending {
    /// The documents the prompt asks about, one at a time.
    pub(crate) guard: CloseGuard<u64>,
    /// The action the prompt holds.
    pub(crate) then: Guarded,
    /// Where the guard stands.
    pub(crate) step: GuardStep<u64>,
    /// The action waits for a Save dialog.
    pub(crate) saving: bool,
}

/// The window's close decision: `Hold` (starting a quit guard) while unsaved work or a held save exists, `Close` once
/// the quit is confirmed.
pub fn close_requested(app: &mut MaterialCraftApp) -> CloseDecision {
    if app.quit_confirmed {
        return CloseDecision::Close;
    }
    if app.unsaved.is_none() && !needs_confirmation(&app.session) {
        return CloseDecision::Close;
    }
    guard(app, Guarded::Quit);
    // An open prompt keeps its own action; a quit guard whose questions all fell away has confirmed the quit
    if app.quit_confirmed { CloseDecision::Close } else { CloseDecision::Hold }
}

/// What the prompt holds, for readers outside the crate.
pub fn held(app: &MaterialCraftApp) -> Option<&Guarded> {
    app.unsaved.as_ref().map(|pending| &pending.then)
}

/// Does throwing this session away lose work?
pub fn needs_confirmation(session: &materialcraft_engine::Session) -> bool {
    session.open_packages().iter().any(|entry| entry.3)
}

/// Whether a held action waits for a Save dialog or a download to answer.
pub(crate) fn saving(app: &MaterialCraftApp) -> bool {
    app.unsaved.as_ref().is_some_and(|pending| pending.saving)
}

/// The prompt's Save started a save that answers later: the held action waits for [`finish_save_dialog`].
pub(crate) fn wait_for_save(app: &mut MaterialCraftApp) {
    if let Some(pending) = &mut app.unsaved {
        pending.saving = true;
    }
}

/// Run `action`, asking first about each modified package it would close. An action already held keeps the prompt
pub fn guard(app: &mut MaterialCraftApp, action: Guarded) {
    if app.unsaved.is_some() {
        return;
    }

    // A graph property typed but not yet committed counts as an edit to ask about.
    crate::properties::GraphPropertyDraft::commit(app);
    let (scope, documents) = match &action {
        Guarded::Package { document, reload } => {
            let scope = if *reload { CloseScope::Revert } else { CloseScope::Close };
            (scope, if modified(app, *document) { vec![*document] } else { Vec::new() })
        }
        Guarded::Menu { .. } | Guarded::Quit => (CloseScope::Quit, modified_documents(app)),
        Guarded::OpenPath { .. } | Guarded::LoadBytes { .. } => (CloseScope::Close, modified_documents(app)),
    };
    let guard = CloseGuard::new(scope, documents);
    let step = guard.current().copied().map_or(GuardStep::Finished, GuardStep::Ask);
    advance(app, Pending { guard, then: action, step, saving: false });
}

fn modified(app: &MaterialCraftApp, document: u64) -> bool {
    app.session.open_packages().iter().any(|entry| entry.0 == document && entry.3)
}

fn modified_documents(app: &MaterialCraftApp) -> Vec<u64> {
    app.session.open_packages().into_iter().filter(|entry| entry.3).map(|entry| entry.0).collect()
}

fn activate(app: &mut MaterialCraftApp, document: u64) -> bool {
    app.run("package.activate", json!({"document": document})).is_some()
}

/// Carry out the guard's steps until one needs the user (a question, a Save dialog, a download) or the guard ends
fn advance(app: &mut MaterialCraftApp, mut pending: Pending) {
    loop {
        pending.step = match pending.step {
            GuardStep::Ask(document) => {
                // The question names the active package; a package that is gone ends the prompt
                if activate(app, document) {
                    app.unsaved = Some(pending);
                } else {
                    app.pending_load = None;
                }
                return;
            }
            GuardStep::Discard(document) => {
                if activate(app, document) {
                    let reload = matches!(pending.then, Guarded::Package { reload: true, .. });
                    app.run(if reload { "file.reload" } else { "package.close" }, json!({"discard": true}));
                }
                pending.guard.discarded()
            }
            GuardStep::Save(document) => {
                if !activate(app, document) {
                    pending.guard.save_failed()
                } else {
                    match save(app, pending, document) {
                        Some(next) => {
                            pending = next;
                            pending.step
                        }
                        None => return,
                    }
                }
            }
            GuardStep::Cancelled => {
                app.pending_load = None;
                return;
            }
            GuardStep::Finished => {
                perform(app, pending.then);
                return;
            }
        };
    }
}

/// Save the active package for the prompt. Returns the guard with its next step when the save finished at once;
/// `None` when it answers later, with the prompt held at `app.unsaved` until [`finish_save_dialog`]
fn save(app: &mut MaterialCraftApp, mut pending: Pending, document: u64) -> Option<Pending> {
    if app.host.files().browser_saves() {
        app.unsaved = Some(pending);
        crate::web_files::PackageSave::save_then(app);
        return None;
    }

    if crate::windows::has_local_path(app) {
        crate::windows::file_action(app, "file.save");
        // A save that leaves the package modified did not save it
        pending.step = if modified(app, document) { pending.guard.save_failed() } else { pending.guard.saved() };
        return Some(pending);
    }

    // One file dialog at a time: a Save that cannot ask stops the action with the package open
    if crate::native_dialogs::pending(app) {
        pending.step = pending.guard.save_failed();
        return Some(pending);
    }

    let archive = app.session.path.as_deref().is_some_and(materialcraft_engine::file_cmds::is_mcraft);
    let name = format!("{}.{}", materialcraft_engine::formats::safe_file_name(&app.session.pkg.name), if archive { "mcraft" } else { "sbs" });
    pending.saving = true;
    app.unsaved = Some(pending);
    let then = crate::native_dialogs::Then::SavePackage { document, copy: false, before_action: true };
    crate::native_dialogs::ask(app, crate::windows::package_dialog(false, archive).file_name(name), then);
    None
}

/// Run an allowed action
pub(crate) fn perform(app: &mut MaterialCraftApp, action: Guarded) {
    match action {
        Guarded::Package { document, reload } => {
            if activate(app, document) {
                app.run(if reload { "file.reload" } else { "package.close" }, json!({"discard": true}));
            }
        }
        action @ (Guarded::Menu { .. } | Guarded::Quit) => {
            // A package a script modified while the prompt was open is asked about too
            if needs_confirmation(&app.session) {
                guard(app, action);
            } else if action == Guarded::Quit {
                app.quit_confirmed = true;
            } else {
                app.run("package.closeAll", Value::Null);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Guarded::OpenPath { path } => crate::windows::open_path_now(app, &path),
        #[cfg(target_arch = "wasm32")]
        Guarded::OpenPath { .. } => {}
        Guarded::LoadBytes { .. } => {
            if let Some((name, bytes)) = app.pending_load.take() {
                crate::web_files::load_now(app, &name, &bytes);
            }
        }
    }
}

/// The Save / Discard / Cancel question for the package the held action asks about
pub fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let Some(pending) = &app.unsaved else { return };
    // While a save runs, the question is not open
    let GuardStep::Ask(document) = pending.step else { return };
    let (scope, remaining) = (pending.guard.scope(), pending.guard.remaining());
    // Activating closes the open sub-editors and the wire selection, so the active package is left as it is
    if document != app.session.document_id && !activate(app, document) {
        app.unsaved = None;
        app.pending_load = None;
        return;
    }

    let name = app.session.pkg.name.clone();
    let mut dialog = UnsavedChangesDialog::new(scope, name.clone()).remaining(remaining).document_wording(true).dont_save_label(tr("Discard"));
    // Reverting keeps the framework's question: it offers no Save
    if scope != CloseScope::Revert {
        dialog = dialog.message(format!("{} \"{name}\"?", tr("Save changes to"))).note(tr("Your changes will be lost if you don't save them."));
    }
    // `app.unsaved`'s Ask step is the open state
    let mut open = true;
    if let Some(answer) = dialog.opened(Bind::bound(&mut open)).show(ui).answer {
        answer_prompt(app, answer);
    }
}

fn answer_prompt(app: &mut MaterialCraftApp, answer: UnsavedAnswer) {
    let Some(mut pending) = app.unsaved.take() else { return };
    pending.step = if answer.apply_to_all { pending.guard.answer_all(answer.answer) } else { pending.guard.answer(answer.answer) };
    advance(app, pending);
}

/// The Save dialog or download of a held action has answered: the guard moves on when `saved`, otherwise the action
/// stops with the package open
pub(crate) fn finish_save_dialog(app: &mut MaterialCraftApp, saved: bool) {
    if !app.unsaved.as_ref().is_some_and(|pending| pending.saving) {
        return;
    }
    let Some(mut pending) = app.unsaved.take() else { return };
    pending.saving = false;
    // A save that leaves the package modified did not save it
    let saved = saved && matches!(pending.step, GuardStep::Save(document) if !modified(app, document));
    pending.step = if saved { pending.guard.saved() } else { pending.guard.save_failed() };
    advance(app, pending);
}

#[cfg(test)]
#[path = "unsaved.test.rs"]
mod tests;
