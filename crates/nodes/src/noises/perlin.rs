use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::FRAC_1_SQRT_2;

pub(crate) fn definition() -> NodeDefinition {
    node("perlin_noise", "Perlin Noise", CATEGORY)
        .desc("Procedural Perlin gradient noise with octave fractal sum and tiling support.")
        .generator()
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("persistence", "Persistence", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .float("disorder", "Disorder", 0.0, 0.0, 100.0)
        .gray_out()
        .cook(cook)
}

const GRADIENTS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [FRAC_1_SQRT_2, FRAC_1_SQRT_2],
    [-FRAC_1_SQRT_2, FRAC_1_SQRT_2],
    [FRAC_1_SQRT_2, -FRAC_1_SQRT_2],
    [-FRAC_1_SQRT_2, -FRAC_1_SQRT_2],
];

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn grad(hash: u32, x: f32, y: f32) -> f32 {
    let g = GRADIENTS[(hash as usize) & 7];
    g[0] * x + g[1] * y
}

#[inline]
fn perlin_2d_periodic(u: f32, v: f32, period_x: i32, period_y: i32, seed: u32) -> f32 {
    let x = (u * period_x as f32).floor() as i32;
    let y = (v * period_y as f32).floor() as i32;

    let fx = u * period_x as f32 - x as f32;
    let fy = v * period_y as f32 - y as f32;

    let x0 = x.rem_euclid(period_x) as u32;
    let x1 = (x + 1).rem_euclid(period_x) as u32;
    let y0 = y.rem_euclid(period_y) as u32;
    let y1 = (y + 1).rem_euclid(period_y) as u32;

    let h00 = crate::random::hash(x0.wrapping_mul(0x9e37_79b9) ^ y0.wrapping_mul(0x85eb_ca6b) ^ seed);
    let h10 = crate::random::hash(x1.wrapping_mul(0x9e37_79b9) ^ y0.wrapping_mul(0x85eb_ca6b) ^ seed);
    let h01 = crate::random::hash(x0.wrapping_mul(0x9e37_79b9) ^ y1.wrapping_mul(0x85eb_ca6b) ^ seed);
    let h11 = crate::random::hash(x1.wrapping_mul(0x9e37_79b9) ^ y1.wrapping_mul(0x85eb_ca6b) ^ seed);

    let n00 = grad(h00, fx, fy);
    let n10 = grad(h10, fx - 1.0, fy);
    let n01 = grad(h01, fx, fy - 1.0);
    let n11 = grad(h11, fx - 1.0, fy - 1.0);

    let wx = fade(fx);
    let wy = fade(fy);

    let nx0 = n00 + wx * (n10 - n00);
    let nx1 = n01 + wx * (n11 - n01);

    nx0 + wy * (nx1 - nx0)
}

pub(crate) fn sample_perlin_fbm(u: f32, v: f32, base_scale: f32, octaves: usize, persistence: f32, lacunarity: f32, seed: u32) -> f32 {
    let mut total = 0.0f32;
    let mut max_amp = 0.0f32;
    let mut amp = 1.0f32;
    let mut freq = base_scale;

    for oct in 0..octaves {
        let period = freq.round().max(1.0) as i32;
        let octave_seed = seed.wrapping_add((oct as u32).wrapping_mul(0x27d4_eb2d));
        let val = perlin_2d_periodic(u, v, period, period, octave_seed);

        total += val * amp;
        max_amp += amp;

        amp *= persistence;
        freq *= lacunarity;
    }

    if max_amp > 0.0 { 0.5 + 0.5 * (total / max_amp) } else { 0.5 }
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let base_scale = context.f("scale").clamp(1.0, 64.0);
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("persistence").clamp(0.0, 1.0);
    let lacunarity = context.f("lacunarity").clamp(1.0, 4.0);
    let disorder = context.f("disorder");

    let seed = ((disorder * 1000.0).round() as i64).rem_euclid(u32::MAX as i64) as u32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let normalized = sample_perlin_fbm(u, v, base_scale, octaves, persistence, lacunarity, seed);
        [normalized.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
