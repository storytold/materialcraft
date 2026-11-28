use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("blue_noise_fast", "Blue Noise Fast", CATEGORY)
        .desc("High-frequency blue noise with high-pass spectral energy and minimal low-frequency clustering.")
        .int("scale", "Scale", 1, 1, 16)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.i("scale").clamp(1, 16) as u32;
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate(w, h, Kind::Gray, |x, y| {
        let bx = x / scale;
        let by = y / scale;

        let sample = |dx: i32, dy: i32| -> f32 {
            let px = (bx as i32 + dx).rem_euclid((w / scale).max(1) as i32) as u32;
            let py = (by as i32 + dy).rem_euclid((h / scale).max(1) as i32) as u32;
            let mut h = px.wrapping_mul(0xcc9e_2d51) ^ py.wrapping_mul(0x1b87_3593) ^ seed;
            h ^= h >> 13;
            h = h.wrapping_mul(0x85eb_ca6b);
            h ^= h >> 16;
            (h as f32) / 4_294_967_295.0
        };

        let center = sample(0, 0);
        let blur = (sample(-1, 0) + sample(1, 0) + sample(0, -1) + sample(0, 1)) * 0.25;
        let val = ((center - blur) * 1.5 + 0.5).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
