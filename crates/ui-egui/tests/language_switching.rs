//! Isolate process-global language switching from tests that construct the UI.
use materialcraft_ui_egui::i18n::{current, is_rtl, set_language, tr, trf};
use std::sync::Mutex;

static LANGUAGE: Mutex<()> = Mutex::new(());

#[test]
fn language_switching_and_arabic_translations_work() {
    let _language = LANGUAGE.lock().unwrap();
    set_language("en");
    assert_eq!(current().code, "en");
    assert_eq!(tr("Properties"), "Properties");
    assert!(!is_rtl());

    set_language("ar");
    assert_eq!(current().code, "ar");
    assert_eq!(tr("Properties"), "الخصائص");
    assert_eq!(tr("Preferences"), "التفضيلات");
    assert_eq!(tr("New graph"), "مخطط جديد");
    assert_eq!(tr("Export outputs"), "تصدير المخرجات");
    assert_eq!(tr("Welcome to MaterialCraft"), "مرحباً بك في MaterialCraft");
    assert!(is_rtl());

    // The update menu item and dialogs use the same catalog
    assert_eq!(tr("Check for updates..."), "التحقق من وجود تحديثات...");
    #[cfg(not(target_arch = "wasm32"))]
    {
        use materialcraft_ui_egui::updates_ui::translate;
        assert_eq!(translate("Cancel", &[]), "إلغاء");
        assert_eq!(translate("{app} update", &[("app", "MaterialCraft")]), "MaterialCraft update");
    }

    // Live formatting
    let formatted = trf("Hello {name}", &[("name", &"World")]);
    assert_eq!(formatted, "Hello World");

    // Fallback for unknown text
    assert_eq!(tr("Unknown untranslated string 1234"), "Unknown untranslated string 1234");

    // Reset to English
    set_language("en");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn startup_uses_a_readable_language_and_keeps_the_chinese_preference() {
    use materialcraft_ui_egui::{MaterialCraftApp, UiState, fonts::UiFonts, i18n::os_language};

    let _language = LANGUAGE.lock().unwrap();
    let mut fonts = UiFonts::default();
    fonts.update(&egui::Context::default(), "zh-Hans", false);
    let mut stored = MemoryStorage::default();
    let mut saved = UiState::default();
    saved.prefs.language = "zh-Hans".into();
    eframe::set_value(&mut stored, "ui", &saved);

    for (storage, requested) in [(None, os_language()), (Some(&stored as &dyn eframe::Storage), "zh-Hans")] {
        let ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = storage;
        let mut app = MaterialCraftApp::new(&cc, materialcraft_engine::new_session());
        // Keep offscreen frames out of the user's thumbnail cache
        app.thumbs = Default::default();
        app.ui.windows.home = false;
        let expected = if requested == "zh-Hans" && !fonts.covers_language(requested) { "en" } else { requested };
        assert_eq!(current().code, expected);

        for preferences_open in [false, true] {
            app.ui.windows.preferences = preferences_open;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.step(ui.ctx());
                assert_eq!(current().code, expected);
                if requested == "zh-Hans" {
                    let label = tr("Preferences");
                    assert_eq!(label, if expected == "en" { "Preferences" } else { "首选项" });
                    assert!(ui.fonts_mut(|f| label.chars().all(|c| f.has_glyph(&egui::FontId::proportional(14.0), c))));
                }
                app.draw(ui);
            })
            .textures_delta
            .clear();
        }

        let mut after = MemoryStorage::default();
        eframe::App::save(&mut app, &mut after);
        assert_eq!(eframe::get_value::<UiState>(&after, "ui").unwrap().prefs.language, requested);
    }
}

#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn flush(&mut self) {}
}
