use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill_mapper", "Flood Fill Mapper", CATEGORY)
        .desc("Maps a custom pattern image onto each flood-filled island aligned with its bounding box and orientation.")
        .color_in("input", "Flood Fill Data")
        .primary_input("input")
        .color_in("pattern", "Pattern")
        .float("rotation_random", "Random Rotation", 0.0, 0.0, 1.0)
        .float("scale", "Scale", 1.0, 0.1, 4.0)
        .int("seed", "Seed", 0, 0, 10000)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let pattern = context.any_or(1, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let rot_rand = context.f("rotation_random").clamp(0.0, 1.0);
    let scale = context.f("scale").clamp(0.01, 10.0);
    let seed = context.i("seed").max(0) as u32;
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let sample = input.sample_axes(u, v, tiling);
        let cu = sample[0];
        let cv = sample[1];
        let bw = sample[2];
        let bh = sample[3];

        if bw <= 1e-5 || bh <= 1e-5 {
            return [0.0, 0.0, 0.0, 0.0];
        }

        // Relative coordinate to centroid with seam wrapping
        let mut du = u - cu;
        let mut dv = v - cv;
        if du > 0.5 {
            du -= 1.0;
        }
        if du < -0.5 {
            du += 1.0;
        }
        if dv > 0.5 {
            dv -= 1.0;
        }
        if dv < -0.5 {
            dv += 1.0;
        }

        let u_bits = (cu * 65536.0).round() as u32;
        let v_bits = (cv * 65536.0).round() as u32;
        let h_rot = crate::random::hash(u_bits.wrapping_mul(0x85eb_ca6b) ^ v_bits.wrapping_mul(0xc2b2_ae35) ^ seed);
        let angle = (h_rot as f32 / 4_294_967_295.0) * rot_rand * 2.0 * PI;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let rx = du * cos_a - dv * sin_a;
        let ry = du * sin_a + dv * cos_a;

        let local_u = (rx / (bw * scale)) + 0.5;
        let local_v = (ry / (bh * scale)) + 0.5;

        if (0.0..=1.0).contains(&local_u) && (0.0..=1.0).contains(&local_v) {
            pattern.sample_axes(local_u, local_v, [false, false])
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    })])
}
