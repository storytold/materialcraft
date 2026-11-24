//! Arabic and Simplified Chinese system-font fallbacks, loaded only when needed.
//!
//! Windows: Microsoft YaHei / Segoe UI and fallbacks.
//! macOS: PingFang / SF Arabic and fallbacks.
//! Linux/BSD: Noto Sans CJK / Noto Sans Arabic and fallbacks.

use std::sync::Arc;

use egui::{FontData, FontFamily};

/// Candidate system font files: (path, face index in a collection).
#[cfg(not(target_arch = "wasm32"))]
fn system_files(chinese: bool) -> Vec<(std::path::PathBuf, u32)> {
    use std::path::PathBuf;
    let mut v: Vec<(PathBuf, u32)> = Vec::new();
    if cfg!(windows) {
        let fonts = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        let files: &[&str] = if chinese { &["msyh.ttc", "simsun.ttc", "simhei.ttf"] } else { &["segoeui.ttf", "tahoma.ttf", "arial.ttf"] };
        v.extend(files.iter().map(|f| (fonts.join(f), 0)));
    } else if cfg!(target_os = "macos") {
        let files: &[&str] = if chinese {
            &["/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc", "/System/Library/Fonts/STHeiti Light.ttc"]
        } else {
            &["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/Supplemental/GeezaPro.ttc"]
        };
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
        v.push((PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"), 0));
    } else if chinese {
        for dir in ["/usr/share/fonts/opentype/noto", "/usr/share/fonts/noto-cjk", "/usr/share/fonts/google-noto-cjk", "/usr/local/share/fonts/noto"] {
            // Noto's CJK collection stores Simplified Chinese in face 2.
            v.push((PathBuf::from(dir).join("NotoSansCJK-Regular.ttc"), 2));
        }
        for file in [
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ] {
            v.push((PathBuf::from(file), 0));
        }
    } else {
        for f in [
            "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansArabic-Regular.ttf",
            "/usr/local/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
        ] {
            v.push((PathBuf::from(f), 0));
        }
    }
    v
}

/// Fallback fonts manager for the egui context.
#[derive(Default)]
pub struct UiFonts {
    /// Attempt each script once, including when no system font is available.
    attempted: [bool; 2],
    data: [Option<Arc<FontData>>; 2],
}

impl UiFonts {
    /// Update fallback fonts according to active language and whether the preferences dialog is open.
    pub fn update(&mut self, ctx: &egui::Context, language: &str, preferences_open: bool) {
        let wanted = [language == "ar" || preferences_open, language == "zh-Hans" || preferences_open];
        let mut changed = false;
        for (index, want) in wanted.into_iter().enumerate() {
            if want && !self.attempted[index] {
                self.attempted[index] = true;
                self.data[index] = Self::load(index == 1);
                changed |= self.data[index].is_some();
            }
        }
        if !changed {
            return;
        }
        let mut defs = crate::theme::font_definitions();
        // Reinstall both loaded scripts so opening the picker or switching languages
        // never removes an earlier fallback. Latin and icons retain their priority.
        for (name, data) in ["script-Arab", "script-Hans"].into_iter().zip(&self.data) {
            if let Some(data) = data {
                defs.font_data.insert(name.to_owned(), data.clone());
                defs.families.entry(FontFamily::Proportional).or_default().push(name.to_owned());
                defs.families.entry(FontFamily::Monospace).or_default().push(name.to_owned());
            }
        }
        ctx.set_fonts(defs);
    }

    /// Whether an Arabic fallback font has been found and loaded.
    pub fn covers_arabic(&self) -> bool {
        self.data[0].is_some()
    }

    /// Whether the language's native name and translated UI can be drawn.
    pub fn covers_language(&self, language: &str) -> bool {
        match language {
            "ar" => self.covers_arabic(),
            "zh-Hans" => self.data[1].is_some(),
            _ => true,
        }
    }

    fn load(chinese: bool) -> Option<Arc<FontData>> {
        #[cfg(not(target_arch = "wasm32"))]
        for (path, index) in system_files(chinese) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() || meta.len() == 0 || meta.len() > (128 << 20) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                let arc = Arc::new(data);
                log::info!("UI font fallback: {}", path.display());
                return Some(arc);
            }
        }
        log::warn!("no system font found for {} text", if chinese { "Simplified Chinese" } else { "Arabic" });
        None
    }
}
