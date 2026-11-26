//! The app's look as the framework resolves it: theme defaults, the root environment and the interface fonts.

use craft_theme::{AppDefaults, AppSizes, ComponentSet, ModeChoice, Size, ThemeId, ThemePair};
use craft_ui::fonts::{I18nScripts, UiFonts};
use craft_ui::runtime::env::{RootEnv, Scale};

use crate::i18n::{self, tr};
use crate::state::Prefs;

/// VectorCraft Modern: the darker and the lighter end of the old brightness levels.
const MODERN: &str = "craft.vectorcraft-modern";
/// VectorCraft Modern Medium: the two middle levels and the app's default.
const MEDIUM: &str = "craft.vectorcraft-modern-medium";

/// A built-in theme's id; an id that fails the form check reads as the suite default rather than panicking.
fn theme(id: &str) -> ThemeId {
    ThemeId::parse(id).unwrap_or_else(|_| ThemeId::suite_default())
}

/// What MaterialCraft shows where the suite has chosen nothing: the Modern Medium theme in both slots, dark, compact.
pub(crate) fn app_defaults() -> AppDefaults {
    AppDefaults { theme: ThemePair::same(theme(MEDIUM)), mode: ModeChoice::Dark, size: Size::Compact, sizes: AppSizes::default(), set: ComponentSet::Creative }
}

/// The old brightness levels as theme and mode choices, for the one-time settings migration, in the order Dark,
/// Medium dark, Medium light, Light.
pub(crate) fn brightness_choices() -> [(ThemePair, ModeChoice); 4] {
    [
        (ThemePair::same(theme(MODERN)), ModeChoice::Dark),
        (ThemePair::same(theme(MEDIUM)), ModeChoice::Dark),
        (ThemePair::same(theme(MEDIUM)), ModeChoice::Light),
        (ThemePair::same(theme(MODERN)), ModeChoice::Light),
    ]
}

/// The names of [`brightness_choices`], in the same order, in the UI language.
pub(crate) fn brightness_labels() -> [&'static str; 4] {
    [tr("Dark"), tr("Medium dark"), tr("Medium light"), tr("Light")]
}

/// The root environment each frame: theme and mode from `prefs.appearance`, density from the prefs, repeating shortcuts.
pub(crate) fn root_env(prefs: &Prefs) -> RootEnv {
    let defaults = app_defaults();
    let choice = &prefs.appearance;
    let pair = choice.theme.clone().unwrap_or(defaults.theme);
    let mode = choice.mode.unwrap_or(defaults.mode);
    // The root holds one theme: a light mode takes the light slot; dark and the OS's mode take the dark one, the
    // app's own fallback mode.
    let id = if mode == ModeChoice::Light { pair.light } else { pair.dark };
    let mut root = RootEnv::new().theme(id).mode(mode).density(choice.size.unwrap_or(defaults.size)).scale(Scale::new(prefs.ui_scale));
    // App shortcuts repeat while held, as a held zoom key keeps zooming.
    root.shortcuts_repeat = true;
    root
}

/// The interface fonts, with the interface language.
pub(crate) fn ui_fonts() -> UiFonts<'static> {
    UiFonts::new(&I18nScripts).ui_language(i18n::current().code)
}
