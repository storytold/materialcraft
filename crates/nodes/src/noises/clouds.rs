use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("clouds", "Clouds", CATEGORY)
        .desc("Soft multi-octave cloud noise with turbulence and contrast shaping.")
        .float("scale", "Scale", 4.0, 1.0, 32.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 1.0, 0.1, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("clouds_1", "Clouds 1", CATEGORY)
        .desc("Standard soft multi-octave Perlin cloud formation.")
        .float("scale", "Scale", 4.0, 1.0, 32.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 1.0, 0.1, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("clouds_2", "Clouds 2", CATEGORY)
        .desc("Billowy puffy cloud variation with rounded inverted billow ridges.")
        .float("scale", "Scale", 4.0, 1.0, 32.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 1.2, 0.1, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_billow)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("clouds_3", "Clouds 3", CATEGORY)
        .desc("Turbulent stormy cloud variation with domain-warped coordinate distortion.")
        .float("scale", "Scale", 4.0, 1.0, 32.0)
        .float("warp", "Turbulence Warp", 0.5, 0.0, 2.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_turbulent)
}

fn cook_billow(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.1, 8.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let val = super::perlin::sample_perlin_fbm(u, v, scale, octaves, persistence, 2.0, seed);
        // Billow transformation
        let billow = 1.0 - (val * 2.0 - 1.0).abs();
        let centered = (billow - 0.5) * contrast + 0.5;
        let final_val = centered.clamp(0.0, 1.0);
        let smooth = final_val * final_val * (3.0 - 2.0 * final_val);
        [smooth, 0.0, 0.0, 1.0]
    })])
}

fn cook_turbulent(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let warp = context.f("warp").clamp(0.0, 4.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let warp_x = super::perlin::sample_perlin_fbm(u, v, scale * 0.5, 3, 0.5, 2.0, seed.wrapping_add(101));
        let warp_y = super::perlin::sample_perlin_fbm(u + 0.3, v + 0.7, scale * 0.5, 3, 0.5, 2.0, seed.wrapping_add(202));

        let wu = u + (warp_x - 0.5) * warp * 0.2;
        let wv = v + (warp_y - 0.5) * warp * 0.2;

        let val = super::perlin::sample_perlin_fbm(wu, wv, scale, octaves, persistence, 2.0, seed);
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.1, 8.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        // Multi-frequency smooth Perlin FBM with soft midpoint
        let val = super::perlin::sample_perlin_fbm(u, v, scale, octaves, persistence, 2.0, seed);
        // Soft contrast around 0.5
        let centered = (val - 0.5) * contrast + 0.5;
        let final_val = centered.clamp(0.0, 1.0);
        // Smoothstep curve for soft cloud edges
        let smooth = final_val * final_val * (3.0 - 2.0 * final_val);

        [smooth, 0.0, 0.0, 1.0]
    })])
}
