use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("directional_noise", "Directional Noise", CATEGORY)
        .desc("Anisotropic stretched noise generator oriented along a customizable angle with stretching and roughness.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("stretch", "Stretch", 8.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("directional_noise_1", "Directional Noise 1", CATEGORY)
        .desc("Fine linear stretched directional noise streaks.")
        .float("scale", "Scale", 32.0, 1.0, 64.0)
        .float("stretch", "Stretch", 16.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("directional_noise_2", "Directional Noise 2", CATEGORY)
        .desc("Multi-frequency directional noise with subtle lateral fluttering.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("stretch", "Stretch", 8.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .int("octaves", "Octaves", 5, 1, 8)
        .float("roughness", "Roughness", 0.6, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("directional_noise_3", "Directional Noise 3", CATEGORY)
        .desc("Heavy high-contrast directional brush and comb streaks.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("stretch", "Stretch", 12.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .int("octaves", "Octaves", 3, 1, 8)
        .float("roughness", "Roughness", 0.7, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_brush)
}

pub(crate) fn definition_4() -> NodeDefinition {
    node("directional_noise_4", "Directional Noise 4", CATEGORY)
        .desc("Flowing wavy directional noise distorted by orthogonal sine waves.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("stretch", "Stretch", 8.0, 1.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .float("wave", "Wave Distortion", 0.2, 0.0, 1.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_wavy)
}

fn cook_brush(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let stretch = context.f("stretch").clamp(1.0, 128.0);
    let angle = context.f("angle") * PI;
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = (x * cos_a - y * sin_a) / stretch + 0.5;
        let ry = (x * sin_a + y * cos_a) + 0.5;

        let raw = super::perlin::sample_perlin_fbm(rx, ry, scale, octaves, persistence, 2.0, seed);
        let val = ((raw - 0.5) * 2.0 + 0.5).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_wavy(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let stretch = context.f("stretch").clamp(1.0, 128.0);
    let angle = context.f("angle") * PI;
    let wave = context.f("wave").clamp(0.0, 2.0);
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let mut rx = (x * cos_a - y * sin_a) / stretch + 0.5;
        let ry = (x * sin_a + y * cos_a) + 0.5;

        rx += (ry * PI * 6.0).sin() * wave * 0.05;

        let val = super::perlin::sample_perlin_fbm(rx, ry, scale, octaves, 0.5, 2.0, seed);
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let stretch = context.f("stretch").clamp(1.0, 128.0);
    let angle = context.f("angle") * PI;
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        // Rotate and stretch along major axis
        let rx = (x * cos_a - y * sin_a) / stretch + 0.5;
        let ry = (x * sin_a + y * cos_a) + 0.5;

        let val = super::perlin::sample_perlin_fbm(rx, ry, scale, octaves, persistence, 2.0, seed);
        [val, 0.0, 0.0, 1.0]
    })])
}
