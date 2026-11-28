use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("normal_normalize", "Normal Normalize", CATEGORY)
        .desc("Re-normalizes tangential normal vectors to exact unit length (X^2 + Y^2 + Z^2 = 1).")
        .color_in("input", "Normal Map")
        .primary_input("input")
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let tiling = context.tiling_axes();

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let nx = p[0] * 2.0 - 1.0;
        let ny = p[1] * 2.0 - 1.0;
        let nz = (p[2] * 2.0 - 1.0).max(0.0);

        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let (rx, ry, rz) = if len > 1e-6 && len.is_finite() { (nx / len, ny / len, nz / len) } else { (0.0, 0.0, 1.0) };

        [rx * 0.5 + 0.5, ry * 0.5 + 0.5, rz * 0.5 + 0.5, p[3]]
    })])
}
