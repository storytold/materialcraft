use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition_1() -> NodeDefinition {
    node("messy_fibers_1", "Messy Fibers 1", CATEGORY)
        .desc("Sparse entangled thread fibers with natural curlings.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("density", "Density", 0.4, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_fibers(ctx, 3, 0.4))
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("messy_fibers_2", "Messy Fibers 2", CATEGORY)
        .desc("Dense felted wool and fabric fiber clumps.")
        .float("scale", "Scale", 20.0, 1.0, 64.0)
        .float("density", "Density", 0.6, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_fibers(ctx, 4, 0.6))
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("messy_fibers_3", "Messy Fibers 3", CATEGORY)
        .desc("Chaotic fine fiberglass and lint strands.")
        .float("scale", "Scale", 28.0, 1.0, 64.0)
        .float("density", "Density", 0.8, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_fibers(ctx, 5, 0.8))
}

fn cook_fibers(context: &CookContext<'_>, passes: usize, default_density: f32) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let density = context.f("density").clamp(0.05, 1.0).max(default_density * 0.5);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut accum = 0.0f32;

        for p in 0..passes {
            let angle = (p as f32 * (PI / (passes as f32))) + (seed as f32 * 0.1);
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            let x = u - 0.5;
            let y = v - 0.5;

            // Stretched coordinate for strand
            let rx = (x * cos_a - y * sin_a) / 12.0 + 0.5;
            let ry = (x * sin_a + y * cos_a) + 0.5;

            let fbm = super::perlin::sample_perlin_fbm(rx, ry, scale, 3, 0.6, 2.0, seed.wrapping_add((p as u32) * 777));
            let threshold = 1.0 - (density * 0.5);
            if fbm > threshold {
                let strand = ((fbm - threshold) / (1.0 - threshold)).powf(2.0);
                accum = accum.max(strand);
            }
        }

        [accum.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
