use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("bnw_spots", "BnW Spots", CATEGORY)
        .desc("High-contrast spot noise generator for dust, plaster, rust pitting, and speckled surface variations.")
        .float("scale", "Scale", 16.0, 1.0, 128.0)
        .float("balance", "Balance", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 2.0, 0.5, 8.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("bnw_spots_1", "BnW Spots 1", CATEGORY)
        .desc("Fine high-frequency speckle and rust pitting spot variation.")
        .float("scale", "Scale", 32.0, 1.0, 128.0)
        .float("balance", "Balance", 0.45, 0.0, 1.0)
        .float("contrast", "Contrast", 3.0, 0.5, 8.0)
        .int("octaves", "Octaves", 5, 1, 8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("bnw_spots_2", "BnW Spots 2", CATEGORY)
        .desc("Medium clustered spot clumps with organic distribution.")
        .float("scale", "Scale", 16.0, 1.0, 128.0)
        .float("balance", "Balance", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 2.5, 0.5, 8.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("bnw_spots_3", "BnW Spots 3", CATEGORY)
        .desc("Multi-scale spots with broad soft patches and fine micro-speckles.")
        .float("scale", "Scale", 8.0, 1.0, 128.0)
        .float("balance", "Balance", 0.55, 0.0, 1.0)
        .float("contrast", "Contrast", 1.8, 0.5, 8.0)
        .int("octaves", "Octaves", 6, 1, 8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 256.0);
    let balance = context.f("balance").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.1, 10.0);
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let fbm = super::perlin::sample_perlin_fbm(u, v, scale, octaves, 0.6, 2.1, seed);
        // Balance controls the threshold point; contrast drives the steepness
        let t = (fbm - (1.0 - balance)) * contrast + 0.5;
        let val = t.clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
