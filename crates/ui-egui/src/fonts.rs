//! Arabic and Georgian fallback fonts for the UI. egui's default fonts have no Arabic or Georgian
//! glyphs, so translated UIs (and layer or file names in those scripts) need fallback fonts. They come from:
//!
//! 1. Georgian: Noto Sans Georgian from storytold/craft-fonts, embedded at build time when
//!    `CRAFT_FONTS_DIR` points at a checkout (`build.rs`; native and web, optional);
//! 2. on desktop, the operating system's fonts.
//!
//! Windows: Segoe UI, Tahoma, Arial (Arabic); Sylfaen, Segoe UI (Georgian)
//! macOS: SF Arabic, Geeza Pro, Arial Unicode (Arabic); SF Georgian, Noto Sans Georgian, Arial Unicode (Georgian)
//! Linux: Noto Sans Arabic, DejaVu Sans; Noto Sans Georgian, DejaVu Sans
//!
//! In the browser only the embedded face can draw Georgian.

use std::sync::Arc;

use egui::{FontData, FontFamily};

include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));

/// The scripts with a fallback font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    Arabic,
    Georgian,
}

impl Script {
    const ALL: [Script; 2] = [Script::Arabic, Script::Georgian];

    /// ISO 15924 code (names the egui font entry, `script-Arab`).
    fn iso(self) -> &'static str {
        match self {
            Script::Arabic => "Arab",
            Script::Georgian => "Geor",
        }
    }

    /// The script a UI language needs beyond Latin.
    fn of_language(code: &str) -> Option<Script> {
        match code {
            "ar" => Some(Script::Arabic),
            "ka" => Some(Script::Georgian),
            _ => None,
        }
    }

    /// Candidate system font files: (path, face index in a collection).
    #[cfg(not(target_arch = "wasm32"))]
    fn system_files(self) -> Vec<(std::path::PathBuf, u32)> {
        use std::path::PathBuf;
        let mut v: Vec<(PathBuf, u32)> = Vec::new();
        if cfg!(windows) {
            let fonts = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
            let files: &[&str] = match self {
                Script::Arabic => &["segoeui.ttf", "tahoma.ttf", "arial.ttf"],
                Script::Georgian => &["sylfaen.ttf", "segoeui.ttf"],
            };
            v.extend(files.iter().map(|f| (fonts.join(f), 0)));
        } else if cfg!(target_os = "macos") {
            let files: &[&str] = match self {
                Script::Arabic => &["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/Supplemental/GeezaPro.ttc"],
                Script::Georgian => &[
                    "/System/Library/Fonts/SFGeorgian.ttf",
                    "/System/Library/Fonts/Supplemental/NotoSansGeorgian-Regular.ttf",
                    "/Library/Fonts/NotoSansGeorgian-Regular.ttf",
                ],
            };
            v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
            v.push((PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"), 0));
        } else {
            let noto = match self {
                Script::Arabic => "NotoSansArabic-Regular.ttf",
                Script::Georgian => "NotoSansGeorgian-Regular.ttf",
            };
            for dir in ["/usr/share/fonts/truetype/noto", "/usr/share/fonts/noto", "/usr/share/fonts/google-noto", "/usr/local/share/fonts/noto"] {
                v.push((PathBuf::from(dir).join(noto), 0));
            }
            for f in
                ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans.ttf", "/usr/local/share/fonts/dejavu/DejaVuSans.ttf"]
            {
                v.push((PathBuf::from(f), 0));
            }
        }
        v
    }

    /// The first available font: the embedded craft-fonts face, then the system's.
    fn load(self) -> Option<Arc<FontData>> {
        if self == Script::Georgian && !GEORGIAN_FONT.is_empty() {
            log::info!("UI font for Georgian: embedded Noto Sans Georgian");
            return Some(Arc::new(FontData::from_static(GEORGIAN_FONT)));
        }
        #[cfg(not(target_arch = "wasm32"))]
        for (path, index) in self.system_files() {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() || meta.len() == 0 || meta.len() > (128 << 20) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                log::info!("UI font fallback for {}: {}", self.iso(), path.display());
                return Some(Arc::new(data));
            }
        }
        log::warn!("no font found for {} text; it may show as boxes", self.iso());
        None
    }
}

/// Fallback fonts manager for the egui context.
#[derive(Default)]
pub struct UiFonts {
    /// Scripts installed in the context, in priority order.
    installed: Vec<Script>,
    /// Font data per script (`None`: nothing found), loaded once.
    loaded: Vec<(Script, Option<Arc<FontData>>)>,
}

impl UiFonts {
    /// Update fallback fonts according to the active language and whether the preferences dialog
    /// (with the language picker) is open, which shows every language's own name.
    pub fn update(&mut self, ctx: &egui::Context, language: &str, preferences_open: bool) {
        let mut want: Vec<Script> = Script::of_language(language).into_iter().collect();
        for s in Script::ALL {
            if (preferences_open || self.installed.contains(&s)) && !want.contains(&s) {
                want.push(s);
            }
        }
        if want.is_empty() || want == self.installed {
            return;
        }
        let mut defs = crate::theme::font_definitions();
        for s in &want {
            let Some(data) = self.data(*s) else { continue };
            let name = format!("script-{}", s.iso());
            defs.font_data.insert(name.clone(), data);
            defs.families.entry(FontFamily::Proportional).or_default().push(name.clone());
            defs.families.entry(FontFamily::Monospace).or_default().push(name);
        }
        ctx.set_fonts(defs);
        self.installed = want;
    }

    /// Whether an Arabic fallback font has been found and loaded.
    pub fn covers_arabic(&self) -> bool {
        self.covers(Script::Arabic)
    }

    /// Whether a Georgian font (embedded or the system's) has been found and loaded.
    pub fn covers_georgian(&self) -> bool {
        self.covers(Script::Georgian)
    }

    fn covers(&self, script: Script) -> bool {
        self.installed.contains(&script) && self.loaded.iter().any(|(s, d)| *s == script && d.is_some())
    }

    fn data(&mut self, script: Script) -> Option<Arc<FontData>> {
        if let Some((_, d)) = self.loaded.iter().find(|(s, _)| *s == script) {
            return d.clone();
        }
        let d = script.load();
        self.loaded.push((script, d.clone()));
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_map_to_scripts() {
        assert_eq!(Script::of_language("ar"), Some(Script::Arabic));
        assert_eq!(Script::of_language("ka"), Some(Script::Georgian));
        assert_eq!(Script::of_language("en"), None);
    }

    #[test]
    fn fonts_install_once_and_keep_the_language_script_first() {
        let ctx = egui::Context::default();
        let mut f = UiFonts::default();
        f.update(&ctx, "en", false);
        assert!(f.installed.is_empty() && f.loaded.is_empty(), "English loads nothing");
        f.update(&ctx, "ka", false);
        assert_eq!(f.installed, [Script::Georgian]);
        f.update(&ctx, "ar", true);
        assert_eq!(f.installed, [Script::Arabic, Script::Georgian]);
        f.update(&ctx, "ar", false);
        assert_eq!(f.loaded.len(), 2, "each script is read at most once");
    }

    /// Runs only when the build embedded craft-fonts' face (CRAFT_FONTS_DIR with Geor): then
    /// Georgian text must draw from it with no system font involved.
    #[test]
    fn embedded_georgian_face_draws_georgian() {
        if GEORGIAN_FONT.is_empty() {
            assert!(option_env!("CRAFT_FONTS_REQUIRED") != Some("1"), "CRAFT_FONTS_REQUIRED=1 builds must embed the face");
            eprintln!("SKIPPED: built without craft-fonts Geor");
            return;
        }
        let ctx = egui::Context::default();
        let mut f = UiFonts::default();
        f.update(&ctx, "ka", false);
        assert!(f.covers_georgian());
        // set_fonts takes effect at the start of the next pass.
        for _ in 0..2 {
            // The pass's texture updates must be taken, or egui panics on drop.
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
        }
        let id = egui::FontId::proportional(14.0);
        assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(&id, "ქართული ენა")), "Georgian glyphs missing");
        assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(&id, "Ab")));
    }
}
