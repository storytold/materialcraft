//! The app's look as the framework resolves it: theme defaults, the root environment and the interface fonts.

use craft_theme::{AppDefaults, ModeChoice, ThemePair};
use craft_ui::fonts::UiFonts;
use craft_ui::runtime::env::RootEnv;

use crate::state::Prefs;

/// What MaterialCraft shows where the suite has chosen nothing: the Modern Medium theme in both slots, dark, compact.
#[allow(dead_code)] // P3(shell-w12): sh.A8
pub(crate) fn app_defaults() -> AppDefaults {
    todo!("sh.A8")
}

/// The old brightness levels as theme and mode choices, for the one-time settings migration.
#[allow(dead_code)] // P3(shell-w12): sh.A8
pub(crate) fn brightness_choices() -> [(ThemePair, ModeChoice); 4] {
    todo!("sh.A8")
}

/// The root environment each frame: theme and mode from `prefs.appearance`, density from the prefs, repeating shortcuts.
#[allow(dead_code)] // P3(shell-w12): sh.A8
pub(crate) fn root_env(prefs: &Prefs) -> RootEnv {
    let _ = prefs;
    todo!("sh.A8")
}

/// The interface fonts, with the interface language.
#[allow(dead_code)] // P3(shell-w12): sh.A8
pub(crate) fn ui_fonts() -> UiFonts<'static> {
    todo!("sh.A8")
}
