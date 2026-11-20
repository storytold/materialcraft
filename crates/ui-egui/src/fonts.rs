//! Arabic and Hebrew fallback fonts for the UI. egui's default fonts lack Arabic and Hebrew
//! glyphs, so translated UIs need fallback fonts from the operating system.
//!
//! Windows: Segoe UI, Tahoma, Arial (plus David for Hebrew)
//! macOS: SF Arabic / Geeza Pro, SF Hebrew / Arial Hebrew / Raanana, Arial Unicode
//! Linux: Noto Sans Arabic / Noto Sans Hebrew, DejaVu Sans, FreeSans

use std::sync::Arc;

use egui::{FontData, FontFamily};

#[derive(Clone, Copy)]
enum Script {
    Arabic,
    Hebrew,
}

impl Script {
    const ALL: [Self; 2] = [Self::Arabic, Self::Hebrew];

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Self::Arabic => "Arabic",
            Self::Hebrew => "Hebrew",
        }
    }

    fn font_id(self) -> &'static str {
        match self {
            Self::Arabic => "script-Arab",
            Self::Hebrew => "script-Hebr",
        }
    }

    fn language_code(self) -> &'static str {
        match self {
            Self::Arabic => "ar",
            Self::Hebrew => "he",
        }
    }
}

/// Candidate system font files for `script`: (path, face index in a collection).
#[cfg(not(target_arch = "wasm32"))]
fn system_files(script: Script) -> Vec<(std::path::PathBuf, u32)> {
    use std::path::PathBuf;
    let mut v: Vec<(PathBuf, u32)> = Vec::new();
    if cfg!(windows) {
        let fonts = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        let files: &[&str] = match script {
            Script::Arabic => &["segoeui.ttf", "tahoma.ttf", "arial.ttf"],
            Script::Hebrew => &["segoeui.ttf", "tahoma.ttf", "arial.ttf", "david.ttf"],
        };
        v.extend(files.iter().map(|f| (fonts.join(f), 0)));
    } else if cfg!(target_os = "macos") {
        let files: &[&str] = match script {
            Script::Arabic => &[
                "/System/Library/Fonts/SFArabic.ttf",
                "/System/Library/Fonts/Supplemental/GeezaPro.ttc",
                "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
                "/Library/Fonts/Arial Unicode.ttf",
            ],
            Script::Hebrew => &[
                "/System/Library/Fonts/SFHebrew.ttf",
                "/System/Library/Fonts/Supplemental/ArialHB.ttc",
                "/System/Library/Fonts/Supplemental/Raanana.ttc",
                "/Library/Fonts/Arial Unicode.ttf",
                "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            ],
        };
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
    } else {
        let files: &[&str] = match script {
            Script::Arabic => &[
                "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/google-noto/NotoSansArabic-Regular.ttf",
                "/usr/local/share/fonts/noto/NotoSansArabic-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
            ],
            Script::Hebrew => &[
                "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/google-noto/NotoSansHebrew-Regular.ttf",
                "/usr/local/share/fonts/noto/NotoSansHebrew-Regular.ttf",
                "/usr/share/fonts/truetype/culmus/NachlieliCLM-Regular.otf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
            ],
        };
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
    }
    v
}

#[derive(Default)]
struct ScriptSlot {
    installed: bool,
    probed: Option<Option<Arc<FontData>>>,
}

impl ScriptSlot {
    fn data(&self) -> Option<&Arc<FontData>> {
        self.probed.as_ref().and_then(|d| d.as_ref())
    }

    fn ensure_loaded(&mut self, script: Script) -> Option<Arc<FontData>> {
        if let Some(cached) = &self.probed {
            return cached.clone();
        }
        let loaded = load_system_font(script);
        self.probed = Some(loaded.clone());
        loaded
    }
}

fn load_system_font(script: Script) -> Option<Arc<FontData>> {
    #[cfg(not(target_arch = "wasm32"))]
    for (path, index) in system_files(script) {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if !meta.is_file() || meta.len() == 0 || meta.len() > (128 << 20) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            let arc = Arc::new(data);
            log::info!("UI font fallback for {}: {}", script.name(), path.display());
            return Some(arc);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = script;
    log::warn!("no font found for {} text; system fonts may show boxes", script.name());
    None
}

/// Fallback fonts manager for the egui context.
#[derive(Default)]
pub struct UiFonts {
    slots: [ScriptSlot; 2],
}

impl UiFonts {
    /// Update fallback fonts according to active language and whether the preferences dialog is open.
    pub fn update(&mut self, ctx: &egui::Context, language: &str, preferences_open: bool) {
        let mut rebuild = false;
        for script in Script::ALL {
            let slot = &mut self.slots[script.index()];
            let wanted = language == script.language_code() || preferences_open;
            if wanted && !slot.installed {
                if slot.ensure_loaded(script).is_some() {
                    rebuild = true;
                }
                slot.installed = true;
            }
        }
        if !rebuild {
            return;
        }
        let mut defs = crate::theme::font_definitions();
        for script in Script::ALL {
            if let Some(data) = self.slots[script.index()].data().cloned() {
                let name = script.font_id().to_string();
                defs.font_data.insert(name.clone(), data);
                defs.families.entry(FontFamily::Proportional).or_default().push(name.clone());
                defs.families.entry(FontFamily::Monospace).or_default().push(name);
            }
        }
        ctx.set_fonts(defs);
    }

    /// Whether an Arabic fallback font has been found and loaded.
    pub fn covers_arabic(&self) -> bool {
        self.slots[Script::Arabic.index()].data().is_some()
    }

    /// Whether a Hebrew fallback font has been found and loaded.
    pub fn covers_hebrew(&self) -> bool {
        self.slots[Script::Hebrew.index()].data().is_some()
    }
}
