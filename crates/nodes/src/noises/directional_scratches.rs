use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("directional_scratches", "Directional Scratches", CATEGORY)
        .desc("High-density directional surface scratches and fine hairline abrasions.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("length", "Length", 8.0, 1.0, 32.0)
        .float("density", "Density", 0.5, 0.0, 1.0)
        .angle("angle", "Angle", 0.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let length = context.f("length").clamp(1.0, 64.0);
    let density = context.f("density").clamp(0.0, 1.0);
    let angle = context.f("angle") * PI;
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = (x * cos_a - y * sin_a) / length + 0.5;
        let ry = (x * sin_a + y * cos_a) + 0.5;

        let base = super::perlin::sample_perlin_fbm(rx, ry, scale, 3, 0.6, 2.5, seed);
        let threshold = 1.0 - (density * 0.4);
        let val = if base > threshold { ((base - threshold) / (1.0 - threshold)).powf(2.0) } else { 0.0 };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
