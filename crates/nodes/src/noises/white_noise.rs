use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("white_noise", "White Noise", CATEGORY)
        .desc("Generates uniform pseudo-random white noise across pixels.")
        .int("scale", "Scale", 1, 1, 16)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let scale = context.i("scale").clamp(1, 16) as u32;
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let bx = x / scale;
        let by = y / scale;

        let h0 = bx.wrapping_mul(0x85eb_ca6b) ^ by.wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_mul(0x27d4_eb2d);
        let h = crate::random::hash(h0);
        let val = h as f32 / 4_294_967_295.0;

        [val, 0.0, 0.0, 1.0]
    })])
}
