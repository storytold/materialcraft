//! Design tokens and egui styling: neutral dark charcoal and studio greys in four brightness
//! levels, flat faces with soft corners, panels as rounded cards and one accent colour.
//! Clean-room palette and widgets for the node-based procedural material workflow.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

use crate::i18n::n_;

/// Settings › Interface › Brightness: the greys of the interface. The 3D and 2D views keep their
/// studio grey in every level (they show a render, not chrome).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Brightness {
    Dark,
    MediumLight,
    Light,
    /// Charcoal, the default. An unknown value (a newer settings file) loads as this (serde wants
    /// it last).
    #[default]
    #[serde(other)]
    MediumDark,
}

impl Brightness {
    pub const ALL: [Brightness; 4] = [Brightness::Dark, Brightness::MediumDark, Brightness::MediumLight, Brightness::Light];

    /// The English label (translated where it is shown).
    pub fn label(self) -> &'static str {
        match self {
            Brightness::Dark => n_("Dark"),
            Brightness::MediumDark => n_("Medium dark"),
            Brightness::MediumLight => n_("Medium light"),
            Brightness::Light => n_("Light"),
        }
    }
}

/// Settings › Interface › Accent color: the one colour for what is on, chosen or focused. Cyan,
/// the default, is the app icon's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Accent {
    Copper,
    Coral,
    Blue,
    Violet,
    Green,
    Amber,
    Pink,
    /// An unknown value (a newer settings file) loads as this (serde wants it last).
    #[default]
    #[serde(other)]
    Cyan,
}

impl Accent {
    pub const ALL: [Accent; 8] = [Accent::Cyan, Accent::Copper, Accent::Coral, Accent::Blue, Accent::Violet, Accent::Green, Accent::Amber, Accent::Pink];

    /// The English label (translated where it is shown).
    pub fn label(self) -> &'static str {
        match self {
            Accent::Cyan => n_("Cyan"),
            Accent::Copper => n_("Copper"),
            Accent::Coral => n_("Coral"),
            Accent::Blue => n_("Blue"),
            Accent::Violet => n_("Violet"),
            Accent::Green => n_("Green"),
            Accent::Amber => n_("Amber"),
            Accent::Pink => n_("Pink"),
        }
    }

    /// (accent, accent_strong) on dark panels and on light ones; the strong one fills buttons
    /// with white lettering. Cyan is MaterialCraft's own; the others are VectorCraft's.
    pub(crate) fn colors(self, dark: bool) -> (Color32, Color32) {
        let (a, strong) = match (self, dark) {
            // The app icon's teal (#178a9c, assets/app-icon/materialcraft.svg).
            (Accent::Cyan, true) => (0x4cc6de, 0x178a9c),
            (Accent::Cyan, false) => (0x0e7489, 0x13788f),
            (Accent::Copper, true) => (0xe58a4e, 0xc2672b),
            (Accent::Copper, false) => (0xae5419, 0xb85c22),
            (Accent::Coral, true) => (0xf06a52, 0xe8573f),
            (Accent::Coral, false) => (0xd9472f, 0xd14a33),
            (Accent::Blue, true) => (0x5b96ff, 0x2f74f0),
            (Accent::Blue, false) => (0x2563eb, 0x1f5ad6),
            (Accent::Violet, true) => (0xa98bff, 0x7c55f0),
            (Accent::Violet, false) => (0x7146dc, 0x6a3fd4),
            (Accent::Green, true) => (0x3fcf8e, 0x1f9d63),
            (Accent::Green, false) => (0x16955a, 0x14874f),
            (Accent::Amber, true) => (0xf5b544, 0xc47f0e),
            (Accent::Amber, false) => (0xb06d06, 0xb06d06),
            (Accent::Pink, true) => (0xf36aa3, 0xdb3f7c),
            (Accent::Pink, false) => (0xcc3672, 0xc22e69),
        };
        (hex(a), hex(strong))
    }
}

const fn hex(s: u32) -> Color32 {
    Color32::from_rgb((s >> 16) as u8, (s >> 8) as u8, s as u8)
}

/// Colors and metrics used across the UI, fetched each frame with [`Tokens::get`]. Never
/// hard-code a colour elsewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Light lettering on dark faces (false for the light brightness levels).
    pub dark: bool,
    /// The window's frame: menu bar, status bar and the margins around the cards.
    pub chrome: Color32,
    /// Card faces: dock panels, tool bar, tool shelf, rail.
    pub panel: Color32,
    /// Insets on a card: list rows, output templates.
    pub row: Color32,
    pub row_hover: Color32,
    /// Selected rows: the panel tinted towards the accent.
    pub row_selected: Color32,
    /// Hairlines inside cards.
    pub separator: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_strong: Color32,
    /// What is on, chosen or focused.
    pub accent: Color32,
    /// Buttons filled with the accent (white lettering).
    pub accent_strong: Color32,
    /// Accent-tinted fills: selected buttons and text, drop targets.
    pub accent_dim: Color32,
    /// Text fields.
    pub field: Color32,
    pub field_border: Color32,
    /// Flat button faces.
    pub button: Color32,
    /// Slider rails and outlines that must show on a card.
    pub button_border: Color32,
    /// The well behind an active tool or a toggled button (accent-tinted).
    pub tool_active: Color32,
    pub danger: Color32,
    /// Warnings and the mask-view marker.
    pub warning: Color32,
    /// The 3D and 2D views' background: a neutral studio grey in every brightness.
    pub viewport: Color32,
    /// Lettering drawn over the views (always on their dark grey).
    pub viewport_text: Color32,
    /// The accent drawn over the views (its dark-panel shade, whatever the brightness).
    pub viewport_accent: Color32,
    pub radius: u8,
}

impl Default for Tokens {
    fn default() -> Self {
        Tokens::for_brightness(Brightness::default()).with_accent(Accent::default())
    }
}

impl Tokens {
    /// The greys of brightness `b`: neutral dark charcoal and studio greys; the accent
    /// colours are placeholders until [`Tokens::with_accent`].
    pub fn for_brightness(b: Brightness) -> Self {
        let base = Tokens {
            dark: true,
            chrome: hex(0x121212),
            panel: hex(0x1c1c1c),
            row: hex(0x171717),
            row_hover: hex(0x272727),
            row_selected: hex(0x322828),
            separator: hex(0x282828),
            text: hex(0xdedede),
            text_dim: hex(0x9c9c9c),
            text_strong: hex(0xffffff),
            accent: hex(0x4cc6de),
            accent_strong: hex(0x168aa3),
            accent_dim: hex(0x5a3e30),
            field: hex(0x141414),
            field_border: hex(0x2e2e2e),
            button: hex(0x282828),
            button_border: hex(0x4c4c4c),
            tool_active: hex(0x323232),
            danger: hex(0xe0554a),
            warning: hex(0xe0b040),
            viewport: hex(0x282828),
            viewport_text: hex(0x969696),
            viewport_accent: hex(0x4cc6de),
            radius: 5,
        };
        let light = Tokens { dark: false, text: hex(0x1a1a1a), text_strong: hex(0x000000), danger: hex(0xc8372d), warning: hex(0x8f5f00), ..base };
        match b {
            Brightness::Dark => base,
            Brightness::MediumDark => Tokens {
                chrome: hex(0x1c1c1c),
                panel: hex(0x282828),
                row: hex(0x222222),
                row_hover: hex(0x343434),
                separator: hex(0x323232),
                text: hex(0xe6e6e6),
                text_dim: hex(0xa4a4a4),
                field: hex(0x1c1c1c),
                field_border: hex(0x3c3c3c),
                button: hex(0x363636),
                button_border: hex(0x585858),
                tool_active: hex(0x3e3e3e),
                ..base
            },
            Brightness::MediumLight => Tokens {
                chrome: hex(0xc8c8c8),
                panel: hex(0xd5d5d5),
                row: hex(0xcccccc),
                row_hover: hex(0xc6c6c6),
                separator: hex(0xbdbdbd),
                text_dim: hex(0x4c4c4c),
                field: hex(0xeeeeee),
                field_border: hex(0x9a9a9a),
                button: hex(0xcccccc),
                button_border: hex(0x8c8c8c),
                tool_active: hex(0xb8b8b8),
                ..light
            },
            Brightness::Light => Tokens {
                chrome: hex(0xe8e8e8),
                panel: hex(0xf4f4f4),
                row: hex(0xeaeaea),
                row_hover: hex(0xe2e2e2),
                separator: hex(0xdcdcdc),
                text_dim: hex(0x565656),
                field: hex(0xffffff),
                field_border: hex(0xc4c4c4),
                button: hex(0xe2e2e2),
                button_border: hex(0xa4a4a4),
                tool_active: hex(0xd4d4d4),
                ..light
            },
        }
    }

    /// These tokens with `accent`: the accent, buttons filled with it, and selected rows, active
    /// tools and selections (the panel tinted towards it).
    pub fn with_accent(self, accent: Accent) -> Self {
        let (a, strong) = accent.colors(self.dark);
        let tint = |k: f32| {
            let mix = |p: u8, q: u8| (f32::from(p) + (f32::from(q) - f32::from(p)) * k).round() as u8;
            Color32::from_rgb(mix(self.panel.r(), a.r()), mix(self.panel.g(), a.g()), mix(self.panel.b(), a.b()))
        };
        Tokens {
            accent: a,
            accent_strong: strong,
            accent_dim: tint(0.4),
            row_selected: tint(if self.dark { 0.24 } else { 0.2 }),
            tool_active: tint(if self.dark { 0.3 } else { 0.24 }),
            viewport_accent: accent.colors(true).0,
            ..self
        }
    }

    /// The tokens [`apply`] installed (the defaults before the first frame).
    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::NULL)).unwrap_or_default()
    }

    pub fn cr(&self) -> CornerRadius {
        CornerRadius::same(self.radius)
    }
}

/// The corner radius of [`card`]s.
pub const CARD_RADIUS: u8 = 8;

/// The tool bar, the tool shelf, the rail and side panels are cards: `body` drawn `padding` inside
/// the panel colour with soft corners, `margin` of the window's frame colour around it.
pub fn card<R>(ui: &mut egui::Ui, t: &Tokens, margin: egui::Margin, padding: egui::Margin, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(t.chrome)
        .inner_margin(margin)
        .show(ui, |ui| {
            egui::Frame::NONE
                .fill(t.panel)
                .corner_radius(CARD_RADIUS)
                .inner_margin(padding)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    body(ui)
                })
                .inner
        })
        .inner
}

/// The app's fonts: egui's defaults and the Phosphor icon font (`fonts::UiFonts` adds CJK).
pub fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts
}

/// Gives the context `fonts` through the framework runtime, which keeps its layout memory in step with them. The
/// framework's chrome (the dock, the update dialogs) also draws in the signature and semibold families; both take the
/// proportional faces, as MaterialCraft has no bold face. A family left out draws missing-glyph boxes.
pub fn load_fonts(ctx: &egui::Context, mut fonts: egui::FontDefinitions) {
    let proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(craft_ui::fonts::FontRef::signature().family(), proportional.clone());
    fonts.families.insert(FontFamily::Name(craft_ui::fonts::SEMIBOLD.into()), proportional);
    craft_ui::fonts::load(ctx, fonts);
}

/// Install the fonts (once, at startup; the app applies the style from its settings).
pub fn install(ctx: &egui::Context) {
    load_fonts(ctx, font_definitions());
}

/// Apply brightness `b` and `accent`: store the tokens for [`Tokens::get`] and style egui with
/// them. Cheap enough to call on a settings change, not meant for every frame.
pub fn apply(ctx: &egui::Context, b: Brightness, accent: Accent) {
    let t = Tokens::for_brightness(b).with_accent(accent);
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, t));
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::new(10.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(11.5, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.slider_width = 120.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.window_margin = egui::Margin::same(8);
        s.animation_time = 0.08;
        let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = t.panel;
        v.window_fill = t.panel;
        v.extreme_bg_color = t.field;
        v.text_edit_bg_color = Some(t.field);
        v.faint_bg_color = t.row;
        v.code_bg_color = t.field;
        v.window_stroke = Stroke::new(1.0, t.separator);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(CARD_RADIUS);
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if t.dark { 120 } else { 60 }) };
        v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.dark { 140 } else { 70 }) };
        v.selection.bg_fill = t.accent_dim;
        v.selection.stroke = Stroke::new(1.0, t.accent);
        v.hyperlink_color = t.accent;
        let w = &mut v.widgets;
        for (widget, fill) in [
            (&mut w.noninteractive, t.panel),
            (&mut w.inactive, t.button),
            (&mut w.hovered, t.row_hover),
            (&mut w.active, t.tool_active),
            (&mut w.open, t.row_hover),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.corner_radius = t.cr();
            widget.fg_stroke = Stroke::new(1.0, t.text);
        }
        // Hovered and pressed lettering, and `RichText::strong` (captions), in the strong colour.
        w.hovered.fg_stroke.color = t.text_strong;
        w.active.fg_stroke.color = t.text_strong;
        w.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
        // Flat buttons: the outline is the face's own colour, so only text fields show it.
        w.inactive.bg_stroke = Stroke::new(1.0, t.button);
        w.hovered.bg_stroke = Stroke::new(1.0, t.field_border);
        w.active.bg_stroke = Stroke::new(1.0, t.accent);
        v.slider_trailing_fill = true;
        v.striped = false;
        s.visuals = v;
    });
}

/// Uppercase panel caption (the strong text colour).
pub fn caption(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(10.5).strong()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_unknown_values_load_the_defaults() {
        for b in Brightness::ALL {
            let json = serde_json::to_value(b).unwrap();
            assert_eq!(serde_json::from_value::<Brightness>(json).unwrap(), b);
        }
        for a in Accent::ALL {
            let json = serde_json::to_value(a).unwrap();
            assert_eq!(serde_json::from_value::<Accent>(json).unwrap(), a);
        }
        assert_eq!(serde_json::to_value(Brightness::MediumDark).unwrap(), "mediumDark");
        assert_eq!(serde_json::from_value::<Brightness>("sepia".into()).unwrap(), Brightness::MediumDark);
        assert_eq!(serde_json::from_value::<Accent>("teal".into()).unwrap(), Accent::Cyan);
    }

    #[test]
    fn every_brightness_and_accent_reads() {
        // Relative luminance (WCAG) and contrast ratio.
        let lum = |c: Color32| {
            let ch = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        };
        let contrast = |a: Color32, b: Color32| {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        for b in Brightness::ALL {
            for a in Accent::ALL {
                let t = Tokens::for_brightness(b).with_accent(a);
                assert_eq!(t.dark, matches!(b, Brightness::Dark | Brightness::MediumDark));
                assert!(contrast(t.text, t.panel) >= 7.0, "{b:?}: text on panel");
                assert!(contrast(t.text_dim, t.panel) >= 4.5, "{b:?}: dim text on panel");
                assert!(contrast(t.text, t.row_selected) >= 4.5, "{b:?} {a:?}: text on a selected row");
                assert!(contrast(t.danger, t.panel) >= 3.0 && contrast(t.warning, t.panel) >= 3.0, "{b:?}: warnings on panel");
                assert!(contrast(t.accent, t.panel) >= 2.5, "{b:?} {a:?}: accent on panel");
                assert!(contrast(Color32::WHITE, t.accent_strong) >= 2.5, "{b:?} {a:?}: white on accent button");
                assert!(contrast(t.viewport_text, t.viewport) >= 3.0, "{b:?}: lettering on the views");
                // Slider rails show on the card.
                assert!(contrast(t.button_border, t.panel) >= 1.6, "{b:?}: rail on panel");
            }
        }
        // The views keep their studio grey.
        assert_eq!(Tokens::for_brightness(Brightness::Light).viewport, Tokens::for_brightness(Brightness::Dark).viewport);
    }
}
