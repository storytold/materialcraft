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
    /// BCP 47 code ("en", "ar").
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

// Old Windows and Android locales can still use `iw` for Hebrew.
// Normalize that legacy tag before language selection or direction lookup.
fn canonical_language_code(code: &str) -> &str {
    if code.eq_ignore_ascii_case("iw") { "he" } else { code }
}

/// Switch the UI language (unknown codes fall back to English).
pub fn set_language(code: &str) {
    let code = canonical_language_code(code);
    CURRENT.store(LANGUAGES.iter().position(|l| l.code == code).unwrap_or(0), Ordering::Relaxed);
}

/// The current UI language.
pub fn current() -> &'static Language {
    LANGUAGES.get(CURRENT.load(Ordering::Relaxed)).unwrap_or(&LANGUAGES[0])
}

/// Returns true if `code` is a right-to-left language (e.g. Arabic `"ar"`).
pub fn is_rtl_code(code: &str) -> bool {
    craft_ui::direction::resolve(canonical_language_code(code), craft_ui::direction::Setting::Language) == craft_ui::direction::Direction::Rtl
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

/// The supported language for a locale such as "ar-EG", "ar-SA" ("en" when none matches).
pub fn language_for_locale(locale: &str) -> &'static str {
    let lang = locale.split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
    match canonical_language_code(lang.as_str()) {
        "ar" => "ar",
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
        assert_eq!(language_for_locale("iw-IL"), "he");
        assert_eq!(language_for_locale("iw_IL.UTF-8"), "he");
        assert_eq!(canonical_language_code("IW"), "he");
        assert!(is_rtl_code("he"));
        assert!(is_rtl_code("iw"));
        assert_eq!(language_for_locale("en-US"), "en");
        assert_eq!(language_for_locale("fr-FR"), "en");
    }

    #[test]
    fn hebrew_table_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in he::TABLE {
            assert!(seen.insert(*k), "duplicate key in he::TABLE: {k:?}");
        }
    }

    #[test]
    fn hebrew_expose_parameter_type_selector_is_localized() {
        use materialcraft_engine::core::ValueType;

        for value_type in ValueType::ALL {
            let source = value_type.label();
            let translated = he::TABLE
                .iter()
                .find(|(key, _)| *key == source)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("Missing Hebrew exposed parameter type: {source}"));
            assert_ne!(translated, source, "Untranslated exposed parameter type: {source}");
            assert!(translated.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c)));
        }
    }
    #[test]
    fn hebrew_placeholders_match_source() {
        for (k, v) in he::TABLE {
            let mut kp: Vec<&str> = k.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
            let mut vp: Vec<&str> = v.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
            kp.sort();
            vp.sort();
            assert_eq!(kp, vp, "mismatched placeholders in {k:?} vs {v:?}");
        }
    }

    #[test]
    fn hebrew_main_menus_and_file_workflow_are_localized() {
        // Prevent untranslated top-level menus and primary file actions
        // from slipping back into the Native Hebrew UI.
        let essential =
            ["File", "Edit", "Tools", "Windows", "Help", "Open...", "Save", "Save as...", "Preferences...", "Snap to grid", "Import vector graphics..."];
        for source in essential {
            let translated = he::TABLE
                .iter()
                .find(|(key, _)| *key == source)
                .map(|(_, translated)| *translated)
                .unwrap_or_else(|| panic!("missing Hebrew menu translation: {source}"));
            assert_ne!(translated, source, "untranslated Native menu item: {source}");
        }
    }

    #[test]
    fn hebrew_home_preferences_and_file_warnings_are_localized() {
        // These appear in Native Home, Preferences, graph editing and open/save feedback.
        let essential = [
            "Function Graph",
            "Template",
            "Package",
            "Undo history (steps)",
            "Preview size while editing",
            "3D pan limit (mesh radii)",
            "3D pan speed",
            "Samples",
            "Edit function graph...",
            "function nodes",
            "Bad function",
            "Opened with",
            "warnings: see the Console",
            "Sent to TextureCraft: find it on the shelf under My smart materials",
        ];
        for source in essential {
            let translated = he::TABLE
                .iter()
                .find(|(key, _)| *key == source)
                .map(|(_, translated)| *translated)
                .unwrap_or_else(|| panic!("missing Hebrew Native Home/Preferences translation: {source}"));
            assert_ne!(translated, source, "untranslated Native Home/Preferences label: {source}");
        }
    }

    #[test]
    fn hebrew_inspector_graph_attributes_are_localized() {
        // All these English source keys already pass through tr() in the Native
        // graph inspector; keep both attribute editors and size controls legible.
        let inspector = ["Identifier", "Label", "Description", "Category", "Author", "Author URL", "Tags", "Group", "User Data", "Width", "Height"];
        for key in inspector {
            let translated =
                he::TABLE.iter().find(|(source, _)| *source == key).map(|(_, value)| *value).unwrap_or_else(|| panic!("missing Hebrew Inspector label: {key}"));
            assert_ne!(translated, key, "untranslated Inspector label: {key}");
            assert!(translated.chars().any(|ch| ('א'..='ת').contains(&ch)), "Inspector label lacks Hebrew glyphs: {key}");
        }
    }

    #[test]
    fn hebrew_native_toolbar_file_export_and_failure_feedback_are_localized() {
        // Actual tr() call-site labels on Native toolbars, file dialogs, and export views.
        let essentials = [
            "Engine",
            "Export outputs... (Ctrl+Shift+E)",
            "New graph... (Ctrl+N)",
            "Open package... (Ctrl+O)",
            "Redo (Ctrl+Y)",
            "full size",
            "threads",
            "Not applied: the active graph or package changed while the file dialog was open",
            "The file dialog could not be shown",
            "placeholder node(s) stand in for nodes MaterialCraft doesn't have yet (see the warnings above)",
            "Couldn't save",
            "Download",
            "Downloads the SBS and resources together in a ZIP.",
            "MaterialCraft Project (*.mcraft)",
            "SBS XML Package (*.sbs)",
            ".mcmat export isn't available yet (#190)",
            "Graph's size",
            "MaterialX export isn't available yet (#190)",
            "files",
            "not available yet",
            "Exported color space",
            "This graph has no outputs",
            "Tile 3x3",
        ];
        for key in essentials {
            let value = he::TABLE
                .iter()
                .find(|(source, _)| *source == key)
                .map(|(_, v)| *v)
                .unwrap_or_else(|| panic!("missing Native Hebrew file/export label: {key}"));
            assert_ne!(value, key, "untranslated Native file/export label: {key}");
            assert!(value.chars().any(|c| ('א'..='ת').contains(&c)), "no Hebrew letters in {key}");
        }
    }

    #[test]
    fn arabic_table_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in ar::TABLE {
            assert!(seen.insert(*k), "duplicate key in ar::TABLE: {k:?}");
        }
    }

    #[test]
    fn placeholders_match_between_key_and_arabic_translation() {
        for (k, v) in ar::TABLE {
            let mut kp: Vec<&str> = k.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
            let mut vp: Vec<&str> = v.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(p, _)| p)).collect();
            kp.sort();
            vp.sort();
            assert_eq!(kp, vp, "mismatched placeholders in {k:?} vs {v:?}");
        }
    }
}
