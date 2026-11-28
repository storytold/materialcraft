use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("caustics", "Caustics", CATEGORY)
        .desc("Simulates underwater light refraction caustics and moving liquid ripples.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("sharpness", "Sharpness", 2.0, 0.5, 6.0)
        .float("time", "Time Phase", 0.0, 0.0, 10.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let sharpness = context.f("sharpness").clamp(0.1, 10.0);
    let time = context.f("time");
    let seed = context.i("seed").max(0) as u32;
    let seed_offset = (seed as f32) * 0.0137;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        // Multi-layered wave interference
        let mut accum = 0.0f32;
        let mut freq = scale;
        let mut amp = 1.0f32;

        for layer in 0..3 {
            let layer_f = layer as f32;
            let ang = layer_f * (PI / 3.0);
            let cos_a = ang.cos();
            let sin_a = ang.sin();

            let su = (u * cos_a - v * sin_a) * freq + time * (0.3 + layer_f * 0.1) + seed_offset;
            let sv = (u * sin_a + v * cos_a) * freq - time * (0.2 + layer_f * 0.1) + seed_offset * 1.3;

            let w1 = (su * PI * 2.0).sin();
            let w2 = (sv * PI * 2.0).sin();

            let wave = (w1 * w2).abs();
            // Inverted caustic filament: bright near wave crest crossings
            let filament = (1.0 - wave).powf(sharpness);
            accum += filament * amp;

            freq *= 1.7;
            amp *= 0.5;
        }

        let val = (accum * 0.6).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}
