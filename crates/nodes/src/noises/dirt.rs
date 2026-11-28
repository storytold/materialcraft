use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("dirt", "Dirt", CATEGORY)
        .desc("Organic multi-layered dirt and smudge noise generator.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.5, 0.2, 5.0)
        .float("grunge", "Grunge", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("dirt_1", "Dirt 1", CATEGORY)
        .desc("Light surface dust and subtle smudges.")
        .float("scale", "Scale", 6.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.2, 0.2, 5.0)
        .float("grunge", "Grunge", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("dirt_2", "Dirt 2", CATEGORY)
        .desc("High-contrast mud splatters and droplets.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.5, 0.2, 8.0)
        .float("grunge", "Grunge", 0.7, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("dirt_3", "Dirt 3", CATEGORY)
        .desc("Weathered cavity grime and crevice buildup.")
        .float("scale", "Scale", 10.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.8, 0.2, 5.0)
        .float("grunge", "Grunge", 0.6, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_4() -> NodeDefinition {
    node("dirt_4", "Dirt 4", CATEGORY)
        .desc("Dried soil patches and caked earth flecks.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.0, 0.2, 5.0)
        .float("grunge", "Grunge", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_5() -> NodeDefinition {
    node("dirt_5", "Dirt 5", CATEGORY)
        .desc("Fine powdery soot and ash surface accumulation.")
        .float("scale", "Scale", 24.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.4, 0.2, 5.0)
        .float("grunge", "Grunge", 0.8, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_gradient() -> NodeDefinition {
    node("dirt_gradient", "Dirt Gradient", CATEGORY)
        .desc("Directional vertical dirt accumulation gradient for wall and base weathering.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("intensity", "Intensity", 1.0, 0.0, 2.0)
        .boolean("invert", "Invert Direction", false)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_gradient)
}

fn cook_gradient(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let intensity = context.f("intensity").clamp(0.0, 4.0);
    let invert = context.b("invert");
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let fbm = super::perlin::sample_perlin_fbm(u, v, scale, 4, 0.6, 2.0, seed);
        let grad = if invert { 1.0 - v } else { v };
        let val = (fbm * grad * intensity).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let contrast = context.f("contrast").clamp(0.1, 10.0);
    let grunge = context.f("grunge").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let fbm_low = super::perlin::sample_perlin_fbm(u, v, scale, 3, 0.5, 2.0, seed);
        let fbm_high = super::perlin::sample_perlin_fbm(u, v, scale * 3.5, 4, 0.7, 2.2, seed.wrapping_add(555));

        // Multiply low-frequency blotches with high-frequency dirt flecks
        let mixed = fbm_low * (1.0 - grunge * 0.4) + (fbm_low * fbm_high) * (grunge * 0.8);
        let val = ((mixed - 0.5) * contrast + 0.5).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
