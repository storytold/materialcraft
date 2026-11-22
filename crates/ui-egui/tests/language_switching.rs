//! Isolate process-global language switching from tests that construct the UI.
// Russian translation © 2026 SuperSfera LLC, MIT OR Apache-2.0
use materialcraft_ui_egui::i18n::{current, is_rtl, set_language, tr, trf};

#[test]
fn language_switching_and_arabic_translations_work() {
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

#[test]
fn language_switching_and_russian_translations_work() {
    set_language("en");
    assert_eq!(current().code, "en");
    assert_eq!(tr("Properties"), "Properties");
    assert!(!is_rtl());

    set_language("ru");
    assert_eq!(current().code, "ru");
    assert_eq!(tr("Properties"), "Свойства");
    assert_eq!(tr("Preferences"), "Настройки");
    assert_eq!(tr("New graph"), "Создать граф");
    assert_eq!(tr("Export outputs"), "Экспортировать выходы");
    assert_eq!(tr("Welcome to MaterialCraft"), "Добро пожаловать в MaterialCraft");
    assert!(!is_rtl());
    assert_eq!(tr("Check for updates..."), "Проверить обновления...");
    assert_eq!(trf("{format} export isn't available yet", &[("format", &"EXR")]), "Экспорт EXR пока недоступен");

    // Fallback for unknown text
    assert_eq!(tr("Unknown untranslated string 1234"), "Unknown untranslated string 1234");

    // Reset to English
    set_language("en");
}
