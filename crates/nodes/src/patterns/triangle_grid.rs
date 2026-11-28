use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("triangle_grid", "Triangle Grid", CATEGORY)
        .desc("Isometric equilateral triangle grid pattern generator with line thickness, bevel, and rotation.")
        .int("tiling", "Tiling", 8, 1, 64)
        .float("gap", "Gap", 0.05, 0.0, 0.4)
        .float("bevel", "Bevel", 0.05, 0.001, 0.3)
        .angle("rotation", "Rotation", 0.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let tiling = context.i("tiling").clamp(1, 64) as f32;
    let gap = context.f("gap").clamp(0.0, 0.45);
    let bevel = context.f("bevel").clamp(0.001, 0.5);
    let rot = context.f("rotation") * PI;

    let cos_r = rot.cos();
    let sin_r = rot.sin();

    let sqrt3 = 3.0f32.sqrt();
    let _r_x = 1.0 / tiling;
    let _r_y = sqrt3 / tiling;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = x * cos_r - y * sin_r + 0.5;
        let ry = x * sin_r + y * cos_r + 0.5;

        // Tri-axial coordinates for hexagonal / triangular grid
        let c1 = (rx * tiling).rem_euclid(1.0);
        let c2 = (rx * 0.5 * tiling + ry * 0.5 * sqrt3 * tiling).rem_euclid(1.0);
        let c3 = (rx * 0.5 * tiling - ry * 0.5 * sqrt3 * tiling).rem_euclid(1.0);

        let d1 = (0.5 - (c1 - 0.5).abs()).max(0.0);
        let d2 = (0.5 - (c2 - 0.5).abs()).max(0.0);
        let d3 = (0.5 - (c3 - 0.5).abs()).max(0.0);

        let min_d = d1.min(d2).min(d3);

        if min_d < gap * 0.5 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            let t = ((min_d - gap * 0.5) / bevel).clamp(0.0, 1.0);
            let val = (t * PI * 0.5).sin();
            [val, 0.0, 0.0, 1.0]
        }
    })])
}
