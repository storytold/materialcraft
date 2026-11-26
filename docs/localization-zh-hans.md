# Simplified Chinese

Choose **Edit > Preferences > General > Language > 简体中文**. First-run locales
`zh`, `zh-CN`, `zh-SG` and explicit `zh-Hans` variants select this catalog.
This automatic selection applies to desktop builds; the browser stays in English
until CJK fonts can be supplied. Existing preferences are preserved. Traditional Chinese locales remain on the
English fallback until a separate catalog is available; an explicit script
takes precedence over the region. Choose **English** in the same picker to revert.

`crates/ui-egui/src/i18n/zh_hans.rs` uses MaterialCraft's existing English-keyed
translation tables. It covers the Arabic catalog's keys, current literal
`tr`/`trf` calls, native node and function names, and their parameter labels.
Missing keys still fall back to English. Package-authored names, descriptions,
document data, command IDs and technical identifiers are not rewritten.
This is not a translation of every engine diagnostic or bundled library package.

## Terminology and maintenance

The wording follows the plain meaning of the English source, with shared terms
aligned with the MIT OR Apache-2.0 TextureCraft and craft-i18n Chinese catalogs.
PhotoCraft's localization guide also informs placeholder, identifier and locale
handling. No proprietary translation files or fonts are included.

| English | Simplified Chinese |
| --- | --- |
| Graph / Function graph | 节点图 / 函数图 |
| Package / Library | 资源包 / 节点库 |
| Node / Input / Output | 节点 / 输入 / 输出 |
| Material / Texture / Map | 材质 / 纹理 / 贴图 |
| Base color / Normal / Roughness | 基础色 / 法线 / 粗糙度 |
| Metallic / Ambient occlusion | 金属度 / 环境光遮蔽 |
| Mask / Blend / Levels | 遮罩 / 混合 / 色阶 |
| Displacement / Tiling | 置换 / 平铺 |
| Preset / Preferences | 预设 / 首选项 |

Keep English keys, `{name}` placeholders, shortcuts, escapes and dialog ellipses
intact. Product names and formats such as MaterialCraft, TextureCraft, ArtCraft,
PBR, RGB, SVG and MaterialX keep their spelling. Built-in library tiles and their
accessible names use the same translated label; searching also accepts English
labels and stable IDs. Labels from user-authored library packages stay unchanged.

MaterialCraft has not cut over to the optional shared craft-i18n build input yet.
This catalog intentionally uses its current tables, as the pre-cutover workflow
in [craft-i18n's integration guide](https://github.com/storytold/craft-i18n/blob/main/docs/integration.md)
allows. Moving the translation runtime and dependency pins is separate work.

## Fonts and platform limits

The desktop shell loads system Chinese fonts lazily, after the existing UI and
icon fonts: Microsoft YaHei/SimSun/SimHei on Windows, PingFang/Hiragino Sans GB/
STHeiti on macOS, and Noto Sans CJK (Simplified Chinese face), WenQuanYi or Droid
fallbacks on Linux/BSD. Opening the language picker loads both script fallbacks;
switching between Arabic and Chinese keeps them installed. Missing fonts are
remembered to avoid repeated disk access and log spam on each frame.

No font assets or build dependencies are added. If the system has no suitable
font, the picker shows the English language name and disables that choice.
When a Chinese font cannot be loaded, the desktop UI displays English and keeps
the Chinese preference. Installing a supported font and restarting restores Chinese.
The existing web shell cannot load system fonts; Chinese glyph delivery there
still needs a separate craft-fonts integration and is not claimed by this change.

The existing i18n tests check locale resolution, duplicate keys and placeholders
for all registered tables. Platform font rendering and screen-reader behavior
need their own verification; passing a build does not establish those results.
