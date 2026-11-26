//! Design tokens and egui styling: the Craft look shared with the sibling apps (cool slate greys in
//! four brightness levels, flat faces with soft corners, panels as rounded cards and one accent
//! colour). Our own palette and widgets; no third-party assets.

use egui::{Color32, CornerRadius};

/// The greys of the interface. The 3D and 2D views keep their studio grey in every level (they
/// show a render, not chrome).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brightness {
    Dark,
    MediumLight,
    Light,
    /// Charcoal, the default.
    #[default]
    MediumDark,
}

/// The one colour for what is on, chosen or focused. Cyan, the default, is the app icon's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Accent {
    Copper,
    Coral,
    Blue,
    Violet,
    Green,
    Amber,
    Pink,
    #[default]
    Cyan,
}

impl Accent {
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
    /// The greys of brightness `b` (VectorCraft's values, mapped onto these names); the accent
    /// colours are placeholders until [`Tokens::with_accent`].
    pub fn for_brightness(b: Brightness) -> Self {
        let base = Tokens {
            dark: true,
            chrome: hex(0x0b0d12),
            panel: hex(0x181b22),
            row: hex(0x13161c),
            row_hover: hex(0x262b35),
            row_selected: hex(0x3a2a2c),
            separator: hex(0x232832),
            text: hex(0xdfe4ec),
            text_dim: hex(0x969faf),
            text_strong: hex(0xffffff),
            accent: hex(0x4cc6de),
            accent_strong: hex(0x168aa3),
            accent_dim: hex(0x5a3e30),
            field: hex(0x0f1116),
            field_border: hex(0x2c323e),
            button: hex(0x282d37),
            button_border: hex(0x4a5363),
            tool_active: hex(0x2f3541),
            danger: hex(0xe0554a),
            warning: hex(0xe0b040),
            viewport: hex(0x2a2a2a),
            viewport_text: hex(0x969696),
            viewport_accent: hex(0x4cc6de),
            radius: 5,
        };
        let light = Tokens { dark: false, text: hex(0x1b1f27), text_strong: hex(0x000000), danger: hex(0xc8372d), warning: hex(0x8f5f00), ..base };
        match b {
            Brightness::Dark => base,
            Brightness::MediumDark => Tokens {
                chrome: hex(0x1b1f27),
                panel: hex(0x272c36),
                row: hex(0x21262f),
                row_hover: hex(0x353c49),
                separator: hex(0x323946),
                text: hex(0xe4e9f1),
                text_dim: hex(0x99a2b2),
                field: hex(0x1a1e26),
                field_border: hex(0x3a414e),
                button: hex(0x39404d),
                button_border: hex(0x5a6476),
                tool_active: hex(0x3d4553),
                ..base
            },
            Brightness::MediumLight => Tokens {
                chrome: hex(0xc4cad4),
                panel: hex(0xd3d8e0),
                row: hex(0xc9cfd8),
                row_hover: hex(0xc5ccd6),
                separator: hex(0xbcc3ce),
                text_dim: hex(0x4a5160),
                field: hex(0xe9ecf1),
                field_border: hex(0x9aa2b0),
                button: hex(0xc9d0da),
                button_border: hex(0x8a93a2),
                tool_active: hex(0xb6bec9),
                ..light
            },
            Brightness::Light => Tokens {
                chrome: hex(0xe6e9ee),
                panel: hex(0xf3f5f8),
                row: hex(0xe9ecf1),
                row_hover: hex(0xe3e7ed),
                separator: hex(0xdde1e8),
                text_dim: hex(0x58606e),
                field: hex(0xffffff),
                field_border: hex(0xc3c9d3),
                button: hex(0xe3e7ed),
                button_border: hex(0xa4abb8),
                tool_active: hex(0xd5dbe3),
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

    /// The tokens stored in the context (the defaults: nothing stores others since the app styles
    /// itself through the framework theme).
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

/// The app's fonts: egui's defaults and the Phosphor icon font.
pub fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts
}

/// Install the fonts.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

/// Uppercase panel caption (the strong text colour).
pub fn caption(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(10.5).strong()
}
