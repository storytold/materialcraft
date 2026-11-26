//! The viewport's data colours: one set of values for every theme.

use craft_theme::{Rgba, Tokens};
use craft_ui::style::TokenGroup;

/// The ink of the 2D and 3D viewports: lettering, axes, bounds, selection, light and grid. `accent` follows the
/// theme's solid accent colour.
#[derive(Clone, Debug, TokenGroup)]
#[group(path = "materialcraft.viewport")]
#[allow(dead_code)] // P3(views-w9): vw.A1
pub(crate) struct ViewportInk {
    /// Lettering over a picture.
    pub text: Rgba,
    /// The shadow copy under the lettering.
    pub text_shadow: Rgba,
    /// The X axis.
    pub axis_x: Rgba,
    /// The Y axis.
    pub axis_y: Rgba,
    /// The Z axis.
    pub axis_z: Rgba,
    /// The bounds box.
    pub bounds: Rgba,
    /// The selection outline.
    pub selection: Rgba,
    /// The light gizmo.
    pub light: Rgba,
    /// The ground grid.
    pub grid: Rgba,
    /// The tile edge and the UV lines.
    pub accent: Rgba,
}

#[allow(dead_code)] // P3(views-w9): vw.A1
impl ViewportInk {
    /// The group's defaults from the resolved tokens.
    fn derive(tokens: &Tokens) -> Self {
        let _ = tokens;
        todo!("vw.A1")
    }
}
