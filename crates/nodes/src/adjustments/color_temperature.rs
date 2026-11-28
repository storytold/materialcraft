use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("color_temperature", "Color Temperature", CATEGORY)
        .desc("Adjusts color temperature (warmth / cool) and color tint (magenta / green) of color images.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("temperature", "Temperature", 0.0, -1.0, 1.0)
        .float("tint", "Tint", 0.0, -1.0, 1.0)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let temp = context.f("temperature").clamp(-1.0, 1.0);
    let tint = context.f("tint").clamp(-1.0, 1.0);
    let tiling = context.tiling_axes();

    // Temperature shifts Red vs Blue
    let r_mult = 1.0 + temp * 0.3;
    let b_mult = 1.0 - temp * 0.3;
    // Tint shifts Green vs Magenta (Red & Blue)
    let g_mult = 1.0 + tint * 0.2;
    let rb_mult = 1.0 - tint * 0.1;

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        [(p[0] * r_mult * rb_mult).clamp(0.0, 1.0), (p[1] * g_mult).clamp(0.0, 1.0), (p[2] * b_mult * rb_mult).clamp(0.0, 1.0), p[3]]
    })])
}
