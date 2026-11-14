//! System fallback fonts for Arabic and Hebrew UI text.
//! Both scripts are requested when the language picker is visible, so their
//! native-script language names can be drawn before the user selects them.
//! No font binaries are bundled with MaterialCraft by this module.

use std::sync::Arc;

use egui::{FontData, FontFamily};

/// Existing system font candidates, ordered by preference; Hebrew and Arabic
/// have separate lists because an Arabic-only face cannot draw Hebrew.
#[cfg(not(target_arch = "wasm32"))]
fn system_files(hebrew: bool) -> Vec<(std::path::PathBuf, u32)> {
    use std::path::PathBuf;
    let mut v: Vec<(PathBuf, u32)> = Vec::new();
    if cfg!(windows) {
        let fonts = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        let files: &[&str] = if hebrew { &["segoeui.ttf", "arial.ttf", "david.ttf", "tahoma.ttf"] } else { &["segoeui.ttf", "tahoma.ttf", "arial.ttf"] };
        v.extend(files.iter().map(|f| (fonts.join(f), 0)));
    } else if cfg!(target_os = "macos") {
        let files: &[&str] = if hebrew {
            &[
                "/System/Library/Fonts/SFHebrew.ttf",
                "/System/Library/Fonts/ArialHB.ttc",
                "/System/Library/Fonts/SFHebrewRounded.ttf",
                "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            ]
        } else {
            &["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/GeezaPro.ttc", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf"]
        };
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
    } else {
        let files: &[&str] = if hebrew {
            &[
                "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/google-noto/NotoSansHebrew-Regular.ttf",
                "/usr/local/share/fonts/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
            ]
        } else {
            &[
                "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/google-noto/NotoSansArabic-Regular.ttf",
                "/usr/local/share/fonts/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
            ]
        };
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
    }
    v
}

/// OS locale tags may include region/encoding suffixes, whereas Preferences
/// normally stores a two-letter language code. Some older systems use the
/// legacy ISO 639 code "iw" for Hebrew instead of "he".
fn matches_language(language: &str, code: &str) -> bool {
    language
        .split(['-', '_', '.'])
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case(code) || (code.eq_ignore_ascii_case("he") && part.eq_ignore_ascii_case("iw")))
}

/// Loaded system faces persist as egui fallbacks when the user changes language.
#[derive(Default)]
pub struct UiFonts {
    attempted_arabic: bool,
    attempted_hebrew: bool,
    arabic: Option<Arc<FontData>>,
    hebrew: Option<Arc<FontData>>,
}

impl UiFonts {
    /// Install a font if its script is selected, or both while the picker is open.
    pub fn update(&mut self, ctx: &egui::Context, language: &str, preferences_open: bool) {
        let want_arabic = matches_language(language, "ar") || preferences_open;
        let want_hebrew = matches_language(language, "he") || preferences_open;
        let mut changed = false;
        if want_arabic && !self.attempted_arabic {
            self.attempted_arabic = true;
            self.arabic = Self::load_system(false);
            changed = true;
        }
        if want_hebrew && !self.attempted_hebrew {
            self.attempted_hebrew = true;
            self.hebrew = Self::load_system(true);
            changed = true;
        }
        if !changed {
            return;
        }

        let mut defs = crate::theme::font_definitions();
        let mut installed_any = false;
        for (name, data) in [("script-Arab", self.arabic.as_ref()), ("script-Hebr", self.hebrew.as_ref())] {
            if let Some(data) = data {
                let name = name.to_string();
                defs.font_data.insert(name.clone(), data.clone());
                defs.families.entry(FontFamily::Proportional).or_default().push(name.clone());
                defs.families.entry(FontFamily::Monospace).or_default().push(name);
                installed_any = true;
            }
        }
        if installed_any {
            ctx.set_fonts(defs);
        }
    }

    /// Whether an Arabic fallback was found and loaded.
    pub fn covers_arabic(&self) -> bool {
        self.arabic.is_some()
    }

    /// Whether a Hebrew fallback was found and loaded.
    pub fn covers_hebrew(&self) -> bool {
        self.hebrew.is_some()
    }

    fn load_system(hebrew: bool) -> Option<Arc<FontData>> {
        #[cfg(not(target_arch = "wasm32"))]
        for (path, index) in system_files(hebrew) {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() || meta.len() == 0 || meta.len() > (128 << 20) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                let name = if hebrew { "Hebrew" } else { "Arabic" };
                log::info!("UI font fallback for {}: {}", name, path.display());
                return Some(Arc::new(data));
            }
        }
        let name = if hebrew { "Hebrew" } else { "Arabic" };
        log::warn!("no font found for {} UI text; system fonts may show boxes", name);
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{UiFonts, system_files};

    #[test]
    fn hebrew_native_font_fallback_loads_from_macos() {
        let candidates = system_files(true);
        assert!(candidates.iter().any(|(path, _)| path.ends_with("SFHebrew.ttf")));
        assert!(candidates.iter().any(|(path, _)| path.ends_with("ArialHB.ttc")));

        let mut fonts = UiFonts::default();
        fonts.update(&egui::Context::default(), "he-IL", false);
        assert!(fonts.covers_hebrew(), "missing Hebrew system UI fallback for he-IL");

        let mut legacy_fonts = UiFonts::default();
        legacy_fonts.update(&egui::Context::default(), "iw_IL.UTF-8", false);
        assert!(legacy_fonts.covers_hebrew(), "missing Hebrew fallback for legacy iw locale");
    }
}

#[cfg(test)]
mod locale_tests {
    use super::matches_language;

    #[test]
    fn fallback_accepts_os_locale_variants_without_matching_other_languages() {
        for locale in ["he", "he-IL", "he_IL.UTF-8", "HE-il", "iw", "iw-IL", "iw_IL.UTF-8", "IW-il"] {
            assert!(matches_language(locale, "he"), "{locale}");
        }
        assert!(matches_language("ar-EG", "ar"));
        assert!(!matches_language("en-US", "he"));
        assert!(!matches_language("hi", "he"));
        assert!(!matches_language("iwi-IL", "he"));
        assert!(!matches_language("iw-IL", "ar"));
        assert!(!matches_language("ar-EG", "he"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod language_switch_tests {
    use super::UiFonts;
    use std::sync::Arc;

    /// A loaded font file is not enough: the active egui font families must
    /// actually be able to draw Hebrew after switching the UI language.
    #[test]
    fn native_hebrew_font_renders_glyphs_in_both_ui_families() {
        let ctx = egui::Context::default();
        let mut fonts = UiFonts::default();
        fonts.update(&ctx, "he-IL", false);
        assert!(fonts.covers_hebrew(), "no Hebrew system font was installed");
        // egui materializes its font cache on the first UI frame.
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        ctx.fonts_mut(|atlas| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                let font_id = egui::FontId::new(18.0, family.clone());
                assert!(atlas.has_glyphs(&font_id, "שמירה פתיחה עברית"), "{family:?} is missing Hebrew UI glyphs");
            }
        });

        // Also probe the selected system face alone. Built-in/default fonts
        // might themselves contain Hebrew and otherwise hide a broken fallback.
        let family = egui::FontFamily::Name("hebrew-system-only".into());
        let mut defs = egui::FontDefinitions::default();
        defs.font_data.insert("hebrew-system-only".into(), fonts.hebrew.as_ref().unwrap().clone());
        defs.families.insert(family.clone(), vec!["hebrew-system-only".into()]);
        let probe = egui::Context::default();
        probe.set_fonts(defs);
        let mut probe_output = probe.run_ui(egui::RawInput::default(), |_| {});
        probe_output.textures_delta.clear();
        probe.fonts_mut(|atlas| {
            assert!(atlas.has_glyphs(&egui::FontId::new(18.0, family), "שמירה פתיחה עברית"), "selected macOS fallback does not actually cover Hebrew glyphs");
        });
    }

    #[test]
    fn switching_languages_preserves_both_script_faces() {
        let ctx = egui::Context::default();
        let mut fonts = UiFonts::default();

        // English alone should not force loading either system script.
        fonts.update(&ctx, "en-US", false);
        assert!(!fonts.covers_arabic());
        assert!(!fonts.covers_hebrew());

        fonts.update(&ctx, "ar-EG", false);
        assert!(fonts.covers_arabic());
        assert!(!fonts.covers_hebrew());
        let first_arabic = fonts.arabic.as_ref().unwrap().clone();

        fonts.update(&ctx, "he-IL", false);
        assert!(fonts.covers_hebrew());
        assert!(fonts.covers_arabic());
        let first_hebrew = fonts.hebrew.as_ref().unwrap().clone();
        assert!(Arc::ptr_eq(&first_arabic, fonts.arabic.as_ref().unwrap()));

        // A non-RTL language must not discard previously loaded fallback faces.
        fonts.update(&ctx, "en-US", false);
        assert!(Arc::ptr_eq(&first_arabic, fonts.arabic.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&first_hebrew, fonts.hebrew.as_ref().unwrap()));

        // Before choosing a script, the language picker must show both names.
        let mut picker_fonts = UiFonts::default();
        picker_fonts.update(&egui::Context::default(), "en-US", true);
        assert!(picker_fonts.covers_hebrew());
        assert!(picker_fonts.covers_arabic());
    }
}
