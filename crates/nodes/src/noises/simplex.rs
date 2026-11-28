use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::TAU;

pub(crate) fn definition() -> NodeDefinition {
    node("simplex_noise", "Simplex Noise", CATEGORY)
        .desc("Generates continuous 2D simplex noise with fractal octaves and seamless tiling.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("persistence", "Persistence", 0.5, 0.0, 1.0)
        .float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn simplex2d(x: f32, y: f32, period: i64, seed: u32) -> f32 {
    const F2: f32 = 0.366_025_4;
    const G2: f32 = 0.211_324_87;

    let s = (x + y) * F2;
    let i = (x + s).floor();
    let j = (y + s).floor();

    let t = (i + j) * G2;
    let x0 = x - (i - t);
    let y0 = y - (j - t);

    let (i1, j1) = if x0 > y0 { (1.0, 0.0) } else { (0.0, 1.0) };

    let x1 = x0 - i1 + G2;
    let y1 = y0 - j1 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let p = period.max(1);
    let ii = (i as i64).rem_euclid(p);
    let jj = (j as i64).rem_euclid(p);

    let grad = |ix: i64, iy: i64| -> (f32, f32) {
        let wx = (ix).rem_euclid(p) as u32;
        let wy = (iy).rem_euclid(p) as u32;
        let h = crate::random::hash(wx.wrapping_mul(0x85eb_ca6b) ^ wy.wrapping_mul(0xc2b2_ae35) ^ seed);
        let angle = (h as f32 / 4_294_967_295.0) * TAU;
        (angle.cos(), angle.sin())
    };

    let (g0x, g0y) = grad(ii, jj);
    let (g1x, g1y) = grad(ii + i1 as i64, jj + j1 as i64);
    let (g2x, g2y) = grad(ii + 1, jj + 1);

    let mut n0 = 0.5 - x0 * x0 - y0 * y0;
    let mut n1 = 0.5 - x1 * x1 - y1 * y1;
    let mut n2 = 0.5 - x2 * x2 - y2 * y2;

    let mut total = 0.0f32;
    if n0 > 0.0 {
        n0 *= n0;
        total += n0 * n0 * (g0x * x0 + g0y * y0);
    }
    if n1 > 0.0 {
        n1 *= n1;
        total += n1 * n1 * (g1x * x1 + g1y * y1);
    }
    if n2 > 0.0 {
        n2 *= n2;
        total += n2 * n2 * (g2x * x2 + g2y * y2);
    }

    total * 70.0
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let scale = context.f("scale").round().max(1.0);
    let octaves = context.i("octaves").clamp(1, 8);
    let persistence = context.f("persistence").clamp(0.0, 1.0);
    let lacunarity = context.f("lacunarity").clamp(1.0, 4.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let mut val = 0.0f32;
        let mut amp = 1.0f32;
        let mut total_amp = 0.0f32;
        let mut freq = scale;

        for oct in 0..octaves {
            let period = freq.round() as i64;
            let s_val = simplex2d(u * freq, v * freq, period, seed.wrapping_add(oct as u32 * 1337));
            val += s_val * amp;
            total_amp += amp;
            amp *= persistence;
            freq = (freq * lacunarity).round();
        }

        let norm = if total_amp > 0.0 { val / total_amp } else { 0.0 };
        let out = (norm * 0.5 + 0.5).clamp(0.0, 1.0);
        [out, 0.0, 0.0, 1.0]
    })])
}
