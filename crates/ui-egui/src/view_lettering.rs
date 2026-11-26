//! Lettering over the 2D and 3D pictures.

use craft_theme::TypeStyle;
use craft_ui::primitives::ContentPainterCx;
use egui::{Align2, Pos2};

use crate::data_colors::ViewportInk;

/// Writes `text` at `at` in `style` and `ink.text`, with a one-point shadow copy first when `shadow`: the one path
/// for lettering over both pictures.
#[allow(dead_code)] // P3(views-w9): vw.A2
pub(crate) fn write(painter: &ContentPainterCx<'_>, ink: &ViewportInk, at: Pos2, anchor: Align2, text: &str, style: TypeStyle, shadow: bool) {
    let _ = (painter, ink, at, anchor, text, style, shadow);
    todo!("vw.A2")
}
