use crate::state::Prefs;

/// The staged preferences and shortcut overrides the open window edits until OK or Apply.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draft {
    /// The staged preferences.
    pub(super) prefs: Prefs,
    /// The staged shortcut overrides.
    pub(super) overrides: craft_keymap::Overrides,
}

impl Draft {
    /// A draft equal to the committed `prefs` and `overrides`.
    pub(super) fn of(prefs: &Prefs, overrides: &craft_keymap::Overrides) -> Draft {
        Draft { prefs: prefs.clone(), overrides: overrides.clone() }
    }
}

/// What the open Preferences window keeps between frames (the open page is `UiState::preferences_page`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct State {
    /// The Shortcuts page's search over command names.
    pub(super) shortcut_search: String,
    /// The Shortcuts page's search over keys.
    pub(super) shortcut_key_search: String,
    /// The Shortcuts page lists only entries with a shortcut.
    pub(super) only_assigned: bool,
    /// The library entry whose shortcut field waits for keys.
    pub(super) capturing: Option<String>,
    /// The staged copy, while the window is open.
    pub(super) draft: Option<Draft>,
    /// The framework preferences window's own state.
    pub(super) window: craft_ui::patterns::preferences::PreferencesState,
}
