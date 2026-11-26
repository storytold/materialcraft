//! Edit ▸ Preferences: the framework's preferences window over a staged copy of the preferences and the node
//! creation shortcuts. Edits change only the copy; OK and Apply write it to `ui.prefs` and `ui.shortcut_overrides`
//! and push it into the session, Cancel (or closing the window) drops it. Language, theme and scale take effect on
//! Apply, never as a preview.

mod general_page;
mod graph_page;
mod library_page;
mod preferences_page;
mod preferences_state;
mod projects_page;
mod shortcuts_page;
mod view3d_page;

use std::ops::RangeInclusive;

use craft_ui::contract::{Bind, Show as _};
use craft_ui::form::{SliderField, SliderFieldLayout};
use craft_ui::patterns::preferences::{PageSpec, PrefValue, PreferencesDraft, PreferencesMode, PreferencesModel, PreferencesWindow};
use egui::Ui;

pub(crate) use preferences_page::PAGES;
pub(crate) use preferences_state::{Draft, State};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::Prefs;

/// The Preferences window, shown on every pass while `ui.windows.preferences` is set: staged mode, where `apply`
/// writes `ui.prefs` and `ui.shortcut_overrides`. The draft is made when the window opens and dropped when it closes.
// The integration tests drive the window through this function with the component harness.
pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if app.ui.preferences.draft.is_none() {
        app.ui.preferences.draft = Some(Draft::of(&app.ui.prefs, &app.ui.shortcut_overrides));
    }

    // The window's state is out of `app` for the call, so its binding and the model's `&mut app` do not overlap
    let mut window = std::mem::take(&mut app.ui.preferences.window);
    window.selected = app.ui.preferences_page.clone();
    let output = PreferencesWindow::new(Bind::bound(&mut window)).mode(PreferencesMode::Staged).show(ui, &mut PrefsDraft::new(app));
    if window.selected != app.ui.preferences_page {
        app.ui.preferences.capturing = None;
        app.ui.preferences_page = window.selected.clone();
    }
    app.ui.preferences.window = window;

    if output.closed.is_some() {
        app.ui.windows.preferences = false;
        app.ui.preferences.draft = None;
        app.ui.preferences.capturing = None;
    }
}

/// Row texts the pages borrow that are made at run time, not English keys.
struct Texts {
    /// The Preview size choice's option ids, one per `preview_resolution::SIZES` entry.
    preview_ids: Vec<String>,
    /// The Preview size choice's option names, in the same order.
    preview_labels: Vec<String>,
    /// Where the autosaved copies go (desktop only).
    #[cfg(not(target_arch = "wasm32"))]
    autosave_help: String,
}

impl Texts {
    fn new() -> Texts {
        let sizes = crate::preview_resolution::SIZES;
        Texts {
            preview_ids: sizes.iter().map(u32::to_string).collect(),
            preview_labels: sizes.iter().map(|size| crate::preview_resolution::label(*size)).collect(),
            #[cfg(not(target_arch = "wasm32"))]
            autosave_help: format!("{} {}", tr("The copies are numbered files in"), crate::user_dir("autosave").display()),
        }
    }
}

/// The window's model for one call of `show`: the rows read and write the draft at `app.ui.preferences.draft`.
pub(crate) struct PrefsDraft<'a> {
    app: &'a mut MaterialCraftApp,
    // Row text includes preview names and an autosave folder path made at run time.
    texts: Texts,
}

impl<'a> PrefsDraft<'a> {
    fn new(app: &'a mut MaterialCraftApp) -> PrefsDraft<'a> {
        PrefsDraft { app, texts: Texts::new() }
    }

    /// The staged preferences; the committed ones while no draft exists, as nothing has been edited then.
    fn prefs(&self) -> &Prefs {
        self.app.ui.preferences.draft.as_ref().map_or(&self.app.ui.prefs, |draft| &draft.prefs)
    }

    fn draft_mut(&mut self) -> &mut Draft {
        let ui = &mut self.app.ui;
        ui.preferences.draft.get_or_insert_with(|| Draft::of(&ui.prefs, &ui.shortcut_overrides))
    }
}

/// The page id a row key starts with (`"general.recent"` is on `"general"`).
fn page_of(key: &str) -> &str {
    key.split_once('.').map_or(key, |(page, _)| page)
}

impl PreferencesModel for PrefsDraft<'_> {
    fn pages(&self) -> Vec<PageSpec<'_>> {
        let prefs = self.prefs();
        vec![
            general_page::page(prefs, &self.texts),
            projects_page::page(prefs),
            graph_page::page(prefs),
            view3d_page::page(prefs),
            library_page::page(prefs),
            shortcuts_page::page(),
        ]
    }

    fn set(&mut self, key: &str, value: PrefValue) {
        let prefs = &mut self.draft_mut().prefs;
        match page_of(key) {
            preferences_page::GENERAL => general_page::set(prefs, key, value),
            preferences_page::PROJECTS => projects_page::set(prefs, key, value),
            preferences_page::GRAPH => graph_page::set(prefs, key, value),
            preferences_page::VIEW3D => view3d_page::set(prefs, key, value),
            preferences_page::LIBRARY => library_page::set(prefs, key, value),
            _ => {}
        }
    }

    fn custom_row(&mut self, ui: &mut Ui, key: &str) -> bool {
        match page_of(key) {
            preferences_page::GENERAL => general_page::custom_row(&mut self.draft_mut().prefs, ui, key),
            preferences_page::PROJECTS => projects_page::custom_row(&mut self.draft_mut().prefs, ui, key),
            preferences_page::VIEW3D => view3d_page::custom_row(&mut self.draft_mut().prefs, ui, key),
            preferences_page::LIBRARY => library_page::custom_row(self.app, ui, key),
            preferences_page::SHORTCUTS => shortcuts_page::rows(self.app, ui),
            _ => false,
        }
    }

    fn custom_page(&mut self, _ui: &mut Ui, _page: &str) -> bool {
        false
    }

    fn reset_page(&mut self, page: &str) {
        let defaults = Prefs::default();
        let draft = self.draft_mut();
        match page {
            preferences_page::GENERAL => general_page::reset(&mut draft.prefs, &defaults),
            preferences_page::PROJECTS => projects_page::reset(&mut draft.prefs, &defaults),
            preferences_page::GRAPH => graph_page::reset(&mut draft.prefs, &defaults),
            preferences_page::VIEW3D => view3d_page::reset(&mut draft.prefs, &defaults),
            preferences_page::LIBRARY => library_page::reset(&mut draft.prefs, &defaults),
            preferences_page::SHORTCUTS => shortcuts_page::reset(&mut draft.overrides),
            _ => {}
        }
    }
}

impl PreferencesDraft for PrefsDraft<'_> {
    fn differs(&self) -> bool {
        let ui = &self.app.ui;
        ui.preferences.draft.as_ref().is_some_and(|draft| draft.prefs != ui.prefs || draft.overrides != ui.shortcut_overrides)
    }

    fn apply(&mut self) -> Result<(), String> {
        let Some(draft) = self.app.ui.preferences.draft.clone() else { return Ok(()) };
        let ui = &mut self.app.ui;
        ui.prefs = draft.prefs;
        ui.shortcut_overrides = draft.overrides;
        crate::i18n::set_language(&ui.prefs.language);
        self.app.apply_prefs();

        // `apply_prefs` bounds some values; the draft starts again from what was committed, so Apply disables itself
        let ui = &mut self.app.ui;
        ui.preferences.draft = Some(Draft::of(&ui.prefs, &ui.shortcut_overrides));
        Ok(())
    }

    fn cancel(&mut self) {
        let ui = &mut self.app.ui;
        ui.preferences.draft = Some(Draft::of(&ui.prefs, &ui.shortcut_overrides));
        ui.preferences.capturing = None;
    }
}

/// A slider and number field over `value` for a custom row, on one line under the row's label. Returns whether the
/// value changed this pass.
pub(super) fn slider(ui: &mut Ui, label: &str, value: &mut f64, range: RangeInclusive<f64>, step: f64, decimals: usize) -> bool {
    let before = *value;
    let _ = SliderField::new(label.to_owned(), Bind::bound(value), range).layout(SliderFieldLayout::Inline).step(step).typed_decimals(decimals).show(ui);
    *value != before
}

/// Edits the default 3D view through `edit`. Only an actual change makes a saved default out of the built-in one.
pub(super) fn edit_view(prefs: &mut Prefs, edit: impl FnOnce(&mut materialcraft_engine::View3d)) {
    let mut view = prefs.default_view3d.clone().unwrap_or_default();
    let before = view.clone();
    edit(&mut view);
    if view != before {
        prefs.default_view3d = Some(view);
    }
}

/// `value` as a whole number held to `range`, for an integer row's edit.
pub(super) fn whole<T: TryFrom<i64>>(value: i64, range: RangeInclusive<i64>) -> Option<T> {
    T::try_from(value.clamp(*range.start(), *range.end())).ok()
}
