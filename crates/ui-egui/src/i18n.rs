//! UI text lookup. Keys are the English strings, so English needs no catalog and a missing translation shows in
//! English. Every user-visible literal goes through [`tr`] (or [`trf`] for text with `{name}` placeholders). The
//! catalogs are craft-i18n's, embedded at build time from `CRAFT_I18N_DIR` (`build.rs`); without it every lookup
//! returns its key. `Prefs::language` picks the language.

use std::fmt::Display;
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use craft_i18n::{I18n, Lang};

include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs"));

/// A language the interface offers.
pub struct Language {
    /// BCP 47 code ("en", "ar").
    pub code: &'static str,
    /// The language's own name, for the picker.
    pub name: &'static str,
}

/// The languages the interface offers; the first is the fallback.
pub static LANGUAGES: [Language; 2] = [Language { code: "en", name: "English" }, Language { code: "ar", name: "العربية" }];

/// The embedded catalogs, offering only [`LANGUAGES`].
static I18N: LazyLock<Arc<I18n>> = LazyLock::new(|| {
    let offered: Vec<&str> = LANGUAGES.iter().map(|language| language.code).collect();
    let (i18n, problems) = I18n::new(CRAFT_I18N, &offered);
    for problem in problems {
        log::warn!("translation catalog: {problem}");
    }
    Arc::new(i18n)
});

/// The UI language, process-wide: every thread draws and formats in the same language.
static CURRENT: RwLock<Lang> = RwLock::new(Lang::EN);

/// The catalogs the Host carries: English keys and the translated tables.
pub(crate) fn host_i18n() -> Arc<I18n> {
    Arc::clone(&*I18N)
}

/// The current UI language as the catalogs name it.
pub(crate) fn lang() -> Lang {
    *CURRENT.read().unwrap_or_else(PoisonError::into_inner)
}

/// The [`LANGUAGES`] the interface fonts, the system's included, can draw; the fallback is always offered.
pub fn offered() -> &'static [&'static Language] {
    static OFFERED: LazyLock<Vec<&'static Language>> = LazyLock::new(|| {
        let stack = crate::appearance::ui_font_stack();
        LANGUAGES.iter().enumerate().filter(|(at, language)| *at == 0 || stack.covers_language(language.code, true)).map(|(_, language)| language).collect()
    });
    &OFFERED
}

/// Switch the UI language (a code the catalogs or the fonts don't offer falls back to English).
pub fn set_language(code: &str) {
    let shown = offered().iter().any(|language| language.code.eq_ignore_ascii_case(code));
    let lang = I18N.from_code(code).filter(|_| shown).unwrap_or(Lang::EN);
    *CURRENT.write().unwrap_or_else(PoisonError::into_inner) = lang;
}

/// The current UI language.
pub fn current() -> &'static Language {
    let code = I18N.code(lang());
    LANGUAGES.iter().find(|language| language.code.eq_ignore_ascii_case(code)).unwrap_or(&LANGUAGES[0])
}

/// `text` in the current language (unchanged when it has no translation).
pub fn tr(text: &str) -> &str {
    I18N.t(lang(), text)
}

/// `template` translated, with each `{name}` replaced by its value in `args`. Values are isolated in right-to-left
/// text, so a Latin name in an Arabic sentence stays where the translator put it.
pub fn trf(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let values: Vec<String> = args.iter().map(|(_, value)| value.to_string()).collect();
    let pairs: Vec<(&str, &str)> = args.iter().zip(&values).map(|((name, _), value)| (*name, value.as_str())).collect();
    craft_i18n::fmt(tr(template), &pairs)
}

/// The operating system's (or browser's) UI language among the offered ones, for the first run; English when none
/// fits.
pub fn os_language() -> &'static str {
    I18N.code(I18N.system_lang())
}
