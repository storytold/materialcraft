use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("gradient_circular", "Gradient Circular", CATEGORY)
        .desc("Generates a radial gradient centered in the canvas.")
        .generator()
        .float("radius", "Radius", 0.5, 0.0, 2.0)
        .int("tiling", "Tiling", 1, 1, 64)
        .boolean("reverse", "Reverse", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let radius = context.f("radius").max(1e-4);
    let tiling = context.i("tiling").max(1) as f32;
    let reverse = context.b("reverse");

    let (w_f, h_f) = (context.w as f32, context.h as f32);

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let u = (x as f32 + 0.5) / w_f;
        let v = (y as f32 + 0.5) / h_f;

        let dx = u - 0.5;
        let dy = v - 0.5;
        let dist = (dx * dx + dy * dy).sqrt();

        let norm_dist = (dist / radius) * tiling;
        let val = norm_dist.rem_euclid(1.0);

        let val = if reverse { 1.0 - val } else { val };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
