//! UI text lookup. Keys are the English strings, so English needs no table and a missing
//! translation falls back to English. Every user-visible literal goes through [`tr`] (or [`trf`]
//! for text with `{name}` placeholders); each language is a static `(English, translation)`
//! table, indexed into a hash map the first time the language is used. `Prefs::language` picks it.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

mod ar;
mod he;

pub struct Language {
    /// BCP 47 code ("en", "ar", "he").
    pub code: &'static str,
    /// The language's own name, for the picker.
    pub name: &'static str,
    /// Its English name (the picker's fallback when no font can draw `name`).
    pub english: &'static str,
    /// `(English, translation)` pairs, in any order.
    pub table: &'static [(&'static str, &'static str)],
}

const COUNT: usize = 3;

pub static LANGUAGES: [Language; COUNT] = [
    Language { code: "en", name: "English", english: "English", table: &[] },
    Language { code: "ar", name: "العربية", english: "Arabic", table: ar::TABLE },
    Language { code: "he", name: "עברית", english: "Hebrew", table: he::TABLE },
];

static MAPS: [OnceLock<HashMap<&'static str, &'static str>>; COUNT] = [const { OnceLock::new() }; COUNT];
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Switch the UI language (unknown codes fall back to English).
pub fn set_language(code: &str) {
    CURRENT.store(LANGUAGES.iter().position(|l| l.code == code).unwrap_or(0), Ordering::Relaxed);
}

/// The current UI language.
pub fn current() -> &'static Language {
    LANGUAGES.get(CURRENT.load(Ordering::Relaxed)).unwrap_or(&LANGUAGES[0])
}

/// Returns true if `code` is a right-to-left language (e.g. Arabic `"ar"`).
pub fn is_rtl_code(code: &str) -> bool {
    craft_ui::direction::resolve(code, craft_ui::direction::Setting::Language) == craft_ui::direction::Direction::Rtl
}

/// Returns true if the current UI language is right-to-left.
pub fn is_rtl() -> bool {
    is_rtl_code(current().code)
}

/// `text` in the current language (unchanged when it has no translation).
pub fn tr(text: &str) -> &str {
    let i = CURRENT.load(Ordering::Relaxed);
    let (Some(lang), Some(map)) = (LANGUAGES.get(i), MAPS.get(i)) else { return text };
    if lang.table.is_empty() {
        return text;
    }
    map.get_or_init(|| lang.table.iter().copied().collect()).get(text).copied().unwrap_or(text)
}

/// Marks a static string for translation where it is defined (gettext's `N_`); it is translated
/// with [`tr`] where it is shown. The key test collects both.
pub const fn n_(text: &'static str) -> &'static str {
    text
}

/// `template` translated, with each `{name}` replaced by its value in `args`.
pub fn trf(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = tr(template);
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| args.iter().find(|(k, _)| *k == &after[..close]).map(|(_, v)| (close, v))) {
            Some((close, v)) => {
                use std::fmt::Write as _;
                let _ = write!(out, "{v}");
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The supported language for a locale such as "ar-EG", "ar-SA", "he-IL" ("en" when none matches).
pub fn language_for_locale(locale: &str) -> &'static str {
    let lang = locale.split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    match lang.as_str() {
        "ar" => "ar",
        "he" | "iw" => "he",
        l => LANGUAGES.iter().find(|x| x.code == l).map_or("en", |x| x.code),
    }
}

/// The operating system's (or browser's) UI language, for the first run.
pub fn os_language() -> &'static str {
    #[cfg(not(target_arch = "wasm32"))]
    let locale = sys_locale::get_locale();
    #[cfg(target_arch = "wasm32")]
    let locale = web_sys::window().and_then(|w| w.navigator().language());
    locale.map_or("en", |l| language_for_locale(&l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection() {
        assert_eq!(language_for_locale("ar"), "ar");
        assert_eq!(language_for_locale("ar-EG"), "ar");
        assert_eq!(language_for_locale("ar_SA.UTF-8"), "ar");
        assert_eq!(language_for_locale("he"), "he");
        assert_eq!(language_for_locale("he-IL"), "he");
        assert_eq!(language_for_locale("he_IL.UTF-8"), "he");
        assert_eq!(language_for_locale("iw"), "he");
        assert_eq!(language_for_locale("en-US"), "en");
        assert_eq!(language_for_locale("fr-FR"), "en");
    }

    #[test]
    fn rtl_direction_matches_language_code() {
        assert!(!is_rtl_code("en"));
        assert!(is_rtl_code("ar"));
        assert!(is_rtl_code("he"));
    }

    #[test]
    fn tables_have_no_duplicate_keys_and_matching_placeholders() {
        for (code, table) in [("ar", ar::TABLE), ("he", he::TABLE)] {
            let mut seen = std::collections::HashSet::new();
            for (k, v) in table {
                assert!(seen.insert(*k), "duplicate key in {code}::TABLE: {k:?}");
                let mut kp: Vec<&str> = k.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
                let mut vp: Vec<&str> = v.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
                kp.sort();
                vp.sort();
                assert_eq!(kp, vp, "mismatched placeholders in {code} {k:?} vs {v:?}");
            }
        }
    }

    #[test]
    fn hebrew_table_covers_arabic_table() {
        let he_keys: std::collections::HashSet<&str> = he::TABLE.iter().map(|(k, _)| *k).collect();
        for (k, _) in ar::TABLE {
            assert!(he_keys.contains(*k), "he::TABLE missing ar::TABLE key: {k:?}");
        }
    }
}
