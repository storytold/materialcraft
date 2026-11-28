# Localization parity

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-17 · **Change:** minor · **Target:** the professional node-based material workflow

Every interface string goes through `crate::i18n::tr` (or is marked with `n_` where it is stored),
so catalogs can be added without touching callers. Today only English ships.

| Language | Code | Status | Notes |
|---|---|---|---|
| English | `en` | Complete | Source language |
| Arabic | `ar` | Complete | Full UI translation table (`crates/ui-egui/src/i18n/ar.rs`), system font fallbacks (Segoe UI / SF Arabic / Noto Sans Arabic), live language picker in Preferences |
| Georgian | `ka` | Complete | Full UI translation table (`crates/ui-egui/src/i18n/ka.rs`), live language picker in Preferences; Noto Sans Georgian from craft-fonts (`CRAFT_FONTS_DIR`, desktop and web) or system fonts on desktop |
| French, German, Spanish, Portuguese (Brazil), Italian | `fr`, `de`, `es`, `pt-br`, `it` | Not started | Planned with the shared craft-i18n catalogs |
| Japanese, Chinese (Simplified), Korean | `ja`, `zh-hans`, `ko` | Not started | Needs the craft-fonts CJK faces (`CRAFT_FONTS_DIR`), as in TextureCraft |

## Fonts for Arabic and Georgian

egui's built-in fonts have no Arabic or Georgian glyphs. `crates/ui-egui/src/fonts.rs` adds fallbacks after
the app's own fonts, loading them when the UI language needs them or the preferences dialog (language
picker) is open:

1. **craft-fonts** (Georgian): `crates/ui-egui/build.rs` embeds Noto Sans Georgian Regular (0.23 MB) from
   `CRAFT_FONTS_DIR/fonts/manifest.txt`, verified against its checksum, in native and web builds.
   `CRAFT_FONTS_REQUIRED=1` makes it mandatory (releases). Without `CRAFT_FONTS_DIR` the face is simply left out (as for the SBS fallback font); a checkout that is set but unusable warns.
2. **System fonts** on desktop. Arabic: Segoe UI / Tahoma / Arial, SF Arabic / Geeza Pro, Noto Sans Arabic
   or DejaVu Sans. Georgian: Sylfaen / Segoe UI, SF Georgian / Noto Sans Georgian, Noto Sans Georgian or
   DejaVu Sans.

A browser page can't read system fonts, so on the web Georgian draws only in builds made with craft-fonts.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | minor | Add complete Georgian (`ka`) UI localization; Noto Sans Georgian embedded from craft-fonts (desktop and web), system fallbacks |
| 2026-11-03 | minor | Add complete Arabic (`ar`) UI localization and font fallbacks |
| 2026-11-02 | major | First version |
