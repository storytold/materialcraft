//! Autosave of a modified package: the framework timer decides when a backup is due, and one Host job writes it.

#[cfg(not(target_arch = "wasm32"))]
use crate::MaterialCraftApp;

/// The autosave schedule of the open package and the one backup write in flight.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct State {
    /// When a backup is due, from the document's revision and modified flag.
    timer: craft_platform::autosave::Autosave,
    /// This run's id: two windows on one package keep separate backups.
    workspace: u64,
    /// The revision the backup in flight holds.
    revision: u64,
    /// The backup in flight: its document and where the job answers.
    job: Option<(u64, std::sync::mpsc::Receiver<Result<(), String>>)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            timer: craft_platform::autosave::Autosave::new(None),
            workspace: std::hash::BuildHasher::hash_one(&std::hash::RandomState::new(), ()),
            revision: 0,
            job: None,
        }
    }
}

impl State {
    /// When the next backup can be due, for `request_repaint_after`, so an idle window still saves; `None` while
    /// nothing waits to be saved.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn deadline(&self) -> Option<std::time::Instant> {
        self.timer.deadline()
    }
}

/// Once per frame: finishes the backup whose job answered, then, every `prefs.autosave_minutes` (0 turns autosave
/// off), writes a modified package's backup on one Host job. A slow disk never holds two package snapshots: while a
/// backup is in flight the timer is not asked again. A failed backup's text goes to the status bar, not a toast.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn tick(app: &mut MaterialCraftApp, now: std::time::Instant) {
    let state = &mut app.autosave;
    if let Some((document, answer)) = &state.job {
        let result = match answer.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the backup job ended without an answer".to_owned()),
        };
        let document = *document;
        state.job = None;
        if let Some(status) = state.timer.finished(document, state.revision, result) {
            app.status = status;
        }
    }

    let state = &mut app.autosave;
    let minutes = app.ui.prefs.autosave_minutes;
    state.timer.set_interval((minutes > 0).then(|| std::time::Duration::from_secs(u64::from(minutes) * 60)));
    let document = craft_platform::autosave::AutosaveDoc { id: app.session.document_id, revision: app.session.revision(), dirty: app.session.modified };
    // `due.discard` is not acted on: the backups stay after a save, as the versions folder keeps them (S-H7).
    let due = state.timer.due(now, [document]);
    if !due.save.contains(&document.id) {
        return;
    }
    let folder = crate::autosave_versions::folder(&crate::user_dir("autosave"), app.session.path.as_deref(), state.workspace, document.id);
    let package = app.session.pkg.clone();
    let keep = app.ui.prefs.autosave_versions.clamp(1, crate::state::Prefs::MAX_AUTOSAVE_VERSIONS);
    let (send, answer) = std::sync::mpsc::channel();
    state.revision = document.revision;
    state.job = Some((document.id, answer));
    app.host.jobs().spawn(
        "materialcraft-autosave",
        Box::new(move || {
            // The kind only: the error text can name the user's folders.
            let result = crate::autosave_versions::save(&folder, &package, keep).map_err(|error| error.kind().to_string());
            match &result {
                Ok(()) => log::info!("Saved a recovery copy"),
                Err(kind) => log::warn!("Could not save a recovery copy ({kind})"),
            }
            let _ = send.send(result);
        }),
    );
}
