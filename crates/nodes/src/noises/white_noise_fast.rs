use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("white_noise_fast", "White Noise Fast", CATEGORY)
        .desc("High-performance pseudo-random white noise generator.")
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let mut h = x.wrapping_mul(0xcc9e_2d51) ^ y.wrapping_mul(0x1b87_3593) ^ seed;
        h ^= h >> 13;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 16;
        let val = (h as f32) / 4_294_967_295.0;

        [val, 0.0, 0.0, 1.0]
    })])
}
