use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill_to_gradient", "Flood Fill to Gradient", CATEGORY)
        .desc("Creates linear gradients across each individual flood-filled island with optional external angle and slope maps.")
        .color_in("input", "Flood Fill Data")
        .primary_input("input")
        .gray_in("angle_map", "Angle Map")
        .gray_in("slope_map", "Slope Map")
        .angle("angle", "Angle", 0.0)
        .float("angle_variation", "Angle Variation", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let angle_map = context.sized(1);
    let slope_map = context.sized(2);

    let base_angle = context.f("angle") * PI;
    let angle_variation = context.f("angle_variation").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();
    let tile = tiling[0] && tiling[1];

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

        let du = ((u - cu + 0.5).rem_euclid(1.0)) - 0.5;
        let dv = ((v - cv + 0.5).rem_euclid(1.0)) - 0.5;

        let u_bits = (cu * 65536.0).round() as u32;
        let v_bits = (cv * 65536.0).round() as u32;
        let h = crate::random::hash(u_bits.wrapping_mul(0x85eb_ca6b) ^ v_bits.wrapping_mul(0xc2b2_ae35));
        let rand_offset = (h as f32 / 4_294_967_295.0) * 2.0 - 1.0;

        // Sample optional external angle map at island centroid
        let ext_angle = angle_map.as_ref().map_or(0.0, |aimg| {
            let px = (cu * context.w as f32).round() as i64;
            let py = (cv * context.h as f32).round() as i64;
            aimg.get_gray(px, py, tile) * 2.0 * PI
        });

        // Sample optional external slope map at island centroid
        let ext_slope = slope_map.as_ref().map_or(1.0, |simg| {
            let px = (cu * context.w as f32).round() as i64;
            let py = (cv * context.h as f32).round() as i64;
            simg.get_gray(px, py, tile).clamp(0.0, 2.0)
        });

        let final_angle = base_angle + ext_angle + angle_variation * rand_offset * PI;

        let cos_a = final_angle.cos();
        let sin_a = final_angle.sin();

        let proj = (du * cos_a + dv * sin_a) * ext_slope;
        let half_extent = (bw.max(1e-4) * cos_a.abs() + bh.max(1e-4) * sin_a.abs()) * 0.5;
        let val = 0.5 + 0.5 * (proj / half_extent.max(1e-4));

        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
