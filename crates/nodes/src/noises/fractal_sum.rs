use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("fractal_sum", "Fractal Sum", CATEGORY)
        .desc("General fractal sum Brownian noise with customizable octaves, persistence, and lacunarity.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("fractal_sum_1", "Fractal Sum 1", CATEGORY)
        .desc("Low-frequency 3-octave fractal sum for broad landscape and terrain contours.")
        .float("scale", "Scale", 4.0, 1.0, 64.0)
        .int("octaves", "Octaves", 3, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("fractal_sum_2", "Fractal Sum 2", CATEGORY)
        .desc("Balanced 5-octave fractal Brownian motion noise.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("octaves", "Octaves", 5, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("fractal_sum_3", "Fractal Sum 3", CATEGORY)
        .desc("High-frequency 7-octave fractal noise for fine grit and rough surfaces.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .int("octaves", "Octaves", 7, 1, 10)
        .float("roughness", "Roughness", 0.6, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.2, 1.0, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_4() -> NodeDefinition {
    node("fractal_sum_4", "Fractal Sum 4", CATEGORY)
        .desc("Multi-scale ridged fractal sum producing sharp ridges and creases.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("octaves", "Octaves", 5, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_ridged)
}

pub(crate) fn definition_base() -> NodeDefinition {
    node("fractal_sum_base", "Fractal Sum Base", CATEGORY)
        .desc("Single-octave fundamental base gradient noise.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_base)
}

fn cook_ridged(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let lacunarity = context.f("lacunarity").clamp(1.0, 5.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let val = super::perlin::sample_perlin_fbm(u, v, scale, octaves, persistence, lacunarity, seed);
        let ridge = 1.0 - (val * 2.0 - 1.0).abs();
        [ridge.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_base(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let val = super::perlin::sample_perlin_fbm(u, v, scale, 1, 0.5, 2.0, seed);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let lacunarity = context.f("lacunarity").clamp(1.0, 5.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let val = super::perlin::sample_perlin_fbm(u, v, scale, octaves, persistence, lacunarity, seed);
        [val, 0.0, 0.0, 1.0]
    })])
}
