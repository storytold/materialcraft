use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("normal_invert", "Normal Invert", CATEGORY)
        .desc("Inverts selected axes of a tangent-space normal map (e.g. flipping Green for DirectX / OpenGL conversion).")
        .color_in("input", "Input")
        .boolean("invert_x", "Invert X (Red)", false)
        .boolean("invert_y", "Invert Y (Green)", true)
        .boolean("invert_z", "Invert Z (Blue)", false)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]).to_color();
    let inv_x = context.b("invert_x");
    let inv_y = context.b("invert_y");
    let inv_z = context.b("invert_z");

    Ok(vec![input.map(Kind::Color, |p| {
        let r = if inv_x && p[0].is_finite() { (1.0 - p[0]).clamp(0.0, 1.0) } else { p[0] };
        let g = if inv_y && p[1].is_finite() { (1.0 - p[1]).clamp(0.0, 1.0) } else { p[1] };
        let b = if inv_z && p[2].is_finite() { (1.0 - p[2]).clamp(0.0, 1.0) } else { p[2] };
        [r, g, b, p[3]]
    })])
}
