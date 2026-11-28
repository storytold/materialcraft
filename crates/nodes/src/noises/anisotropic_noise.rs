use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("anisotropic_noise", "Anisotropic Noise", CATEGORY)
        .desc("Directional high-frequency anisotropic noise streaks with angle, scale, and stretching controls.")
        .float("scale", "Scale", 16.0, 1.0, 128.0)
        .float("stretch", "Stretch", 16.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .float("smoothness", "Smoothness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 256.0);
    let stretch = context.f("stretch").clamp(1.0, 128.0);
    let angle = context.f("angle") * PI;
    let smoothness = context.f("smoothness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = (x * cos_a - y * sin_a) / stretch + 0.5;
        let ry = (x * sin_a + y * cos_a) + 0.5;

        let octaves = (smoothness * 3.0).round() as usize + 2;
        let val = super::perlin::sample_perlin_fbm(rx, ry, scale, octaves, 0.7, 2.0, seed);
        [val, 0.0, 0.0, 1.0]
    })])
}
