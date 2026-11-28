use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::TAU;

pub(crate) fn definition() -> NodeDefinition {
    node("turbulence", "Turbulence", CATEGORY)
        .desc("Generates billowy turbulence noise with creased ridges using absolute-value fractal summing.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("octaves", "Octaves", 5, 1, 8)
        .float("persistence", "Persistence", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn sample_noise(x: f32, y: f32, period: i64, seed: u32) -> f32 {
    let p = period.max(1);
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = x - x.floor();
    let fy = y - y.floor();

    let grad = |cx: i64, cy: i64| -> (f32, f32) {
        let wx = cx.rem_euclid(p) as u32;
        let wy = cy.rem_euclid(p) as u32;
        let h = crate::random::hash(wx.wrapping_mul(0x85eb_ca6b) ^ wy.wrapping_mul(0xc2b2_ae35) ^ seed);
        let angle = (h as f32 / 4_294_967_295.0) * TAU;
        (angle.cos(), angle.sin())
    };

    let (g00_x, g00_y) = grad(ix, iy);
    let (g10_x, g10_y) = grad(ix + 1, iy);
    let (g01_x, g01_y) = grad(ix, iy + 1);
    let (g11_x, g11_y) = grad(ix + 1, iy + 1);

    let d00 = g00_x * fx + g00_y * fy;
    let d10 = g10_x * (fx - 1.0) + g10_y * fy;
    let d01 = g01_x * fx + g01_y * (fy - 1.0);
    let d11 = g11_x * (fx - 1.0) + g11_y * (fy - 1.0);

    // Quintic Hermite interpolant: 6t^5 - 15t^4 + 10t^3
    let sx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let sy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);

    let top = d00 * (1.0 - sx) + d10 * sx;
    let bottom = d01 * (1.0 - sx) + d11 * sx;
    top * (1.0 - sy) + bottom * sy
}

pub(crate) fn sample_turbulence_fbm(u: f32, v: f32, scale: f32, octaves: usize, persistence: f32, seed: u32) -> f32 {
    let mut val = 0.0f32;
    let mut amp = 1.0f32;
    let mut total_amp = 0.0f32;
    let mut freq = scale;

    for oct in 0..octaves {
        let period = freq.round() as i64;
        let n_val = sample_noise(u * freq, v * freq, period, seed.wrapping_add(oct as u32 * 1337)).abs();
        val += n_val * amp;
        total_amp += amp;
        amp *= persistence;
        freq = (freq * 2.0).round();
    }

    let norm = if total_amp > 0.0 { val / total_amp } else { 0.0 };
    (norm * 1.6).clamp(0.0, 1.0)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let scale = context.f("scale").round().max(1.0);
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("persistence").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let out = sample_turbulence_fbm(u, v, scale, octaves, persistence, seed);
        [out, 0.0, 0.0, 1.0]
    })])
}
