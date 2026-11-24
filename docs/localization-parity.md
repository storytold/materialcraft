# Localization parity

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-20 · **Change:** minor (Russian interface); previously major (first version) · **Target:** the professional node-based material workflow

Every interface string goes through `crate::i18n::tr` (or is marked with `n_` where it is stored),
so catalogs can be added without touching callers. Today only English ships.

| Language | Code | Status | Notes |
|---|---|---|---|
| English | `en` | Complete | Source language |
| Arabic | `ar` | Complete | Full UI translation table (`crates/ui-egui/src/i18n/ar.rs`), system font fallbacks (Segoe UI / SF Arabic / Noto Sans Arabic), live language picker in Preferences |
| Russian | `ru` | Complete | Full UI translation table (`crates/ui-egui/src/i18n/ru.rs`); Cyrillic is drawn by egui's default fonts, no fallback needed |
| French, German, Spanish, Portuguese (Brazil), Italian | `fr`, `de`, `es`, `pt-br`, `it` | Not started | Planned with the shared craft-i18n catalogs |
| Japanese, Chinese (Simplified), Korean | `ja`, `zh-hans`, `ko` | Not started | Needs the craft-fonts CJK faces (`CRAFT_FONTS_DIR`), as in TextureCraft |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-20 | minor | Add Russian (`ru`) UI localization, including labels that reach `tr` from catalogs and enums (menus, node parameters, library categories, 3D view options, updater dialog), and labels the code drew without `tr` (base parameters, function nodes and ports, node descriptions, counts and size lines); `trc` for words with two meanings; tests pinned to English with `UiState::english()` |
| 2026-11-03 | minor | Add complete Arabic (`ar`) UI localization and font fallbacks |
| 2026-11-02 | major | First version |
