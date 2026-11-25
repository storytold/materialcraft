# Localization parity

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** major (first version) · **Target:** the professional node-based material workflow

Every interface string goes through `crate::i18n::tr` (or is marked with `n_` where it is stored),
so catalogs can be added without touching callers. English and Arabic are available; the first Hebrew catalog is partial and marked for further review.

| Language | Code | Status | Notes |
|---|---|---|---|
| English | `en` | Complete | Source language |
| Arabic | `ar` | Complete | Full UI translation table (`crates/ui-egui/src/i18n/ar.rs`), system font fallbacks (Segoe UI / SF Arabic / Noto Sans Arabic), live language picker in Preferences |
| Hebrew | `he` | Partial draft | Initial 222 original translations of English UI labels in `crates/ui-egui/src/i18n/he.rs`, RTL locale registration; remaining labels use English. Native font fallback and visual desktop/web QA are pending. |
| French, German, Spanish, Portuguese (Brazil), Italian | `fr`, `de`, `es`, `pt-br`, `it` | Not started | Planned with the shared craft-i18n catalogs |
| Japanese, Chinese (Simplified), Korean | `ja`, `zh-hans`, `ko` | Not started | Needs the craft-fonts CJK faces (`CRAFT_FONTS_DIR`), as in TextureCraft |

## Revision history

| 2026-11-11 | minor | Start partial Hebrew (`he`) interface catalog (222 labels), locale and RTL checks; native font/visual verification still pending |
| Date | Change | Summary |
|---|---|---|
| 2026-11-03 | minor | Add complete Arabic (`ar`) UI localization and font fallbacks |
| 2026-11-02 | major | First version |
