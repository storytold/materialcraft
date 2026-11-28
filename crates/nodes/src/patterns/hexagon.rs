use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("hexagon", "Hexagon", CATEGORY)
        .desc("Generates a regular hexagonal grid / honeycomb pattern with controllable tile spacing.")
        .float("tiling", "Tiling", 8.0, 1.0, 64.0)
        .float("gap", "Gap", 0.05, 0.0, 0.5)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let tiling = context.f("tiling").max(1.0);
    let gap = context.f("gap").clamp(0.0, 0.5);

    // Regular hexagon aspect ratio: sqrt(3)
    let sqrt3 = 3.0_f32.sqrt();
    let r = [1.0, sqrt3];
    let h = [0.5, sqrt3 * 0.5];

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        // Scale UV coordinates for periodic hexagonal tiling
        let px = u * tiling;
        let py = v * tiling * sqrt3;

        // Two competing grid centers
        let a_x = (px / r[0]).rem_euclid(1.0) - 0.5;
        let a_y = (py / r[1]).rem_euclid(1.0) - 0.5;

        let b_x = ((px - h[0]) / r[0]).rem_euclid(1.0) - 0.5;
        let b_y = ((py - h[1]) / r[1]).rem_euclid(1.0) - 0.5;

        let dist_a = a_x * a_x * r[0] * r[0] + a_y * a_y * r[1] * r[1];
        let dist_b = b_x * b_x * r[0] * r[0] + b_y * b_y * r[1] * r[1];

        let (gx, gy) = if dist_a < dist_b { (a_x * r[0], a_y * r[1]) } else { (b_x * r[0], b_y * r[1]) };

        // Hexagon edge distance: max(|x| * sqrt(3)/2 + |y| * 0.5, |y|)
        let d = (gx.abs() * (sqrt3 * 0.5) + gy.abs() * 0.5).max(gy.abs());
        let max_radius = 0.5;
        let edge_bound = max_radius * (1.0 - gap);

        let val = if d <= edge_bound { 1.0 } else { 0.0 };
        [val, 0.0, 0.0, 1.0]
    })])
}
