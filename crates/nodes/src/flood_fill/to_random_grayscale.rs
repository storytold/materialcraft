use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill_to_random_grayscale", "Flood Fill to Random Grayscale", CATEGORY)
        .desc("Assigns a flat random grayscale value to each flood-filled island.")
        .color_in("input", "Flood Fill Data")
        .primary_input("input")
        .int("seed", "Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let seed = context.i("seed").max(0) as u32;
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let sample = input.sample_axes(u, v, tiling);
        let cu = sample[0];
        let cv = sample[1];
        let bw = sample[2];
        let bh = sample[3];

        // Background
        if bw == 0.0 && bh == 0.0 && cu == 0.0 && cv == 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let u_bits = (cu * 65536.0).round() as u32;
        let v_bits = (cv * 65536.0).round() as u32;
        let h1 = crate::random::hash(u_bits.wrapping_mul(0x85eb_ca6b) ^ v_bits.wrapping_mul(0xc2b2_ae35) ^ seed);

        let val = (h1 as f32 / 4_294_967_295.0).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}
