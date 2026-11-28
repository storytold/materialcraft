use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("tile_random", "Tile Random", CATEGORY)
        .desc("Randomized grid tile generator with per-tile rotation, random scale, missing tiles, and luminance variation.")
        .int("x_amount", "X Amount", 8, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .float("gap", "Gap", 0.05, 0.0, 0.5)
        .float("bevel", "Bevel", 0.1, 0.0, 0.5)
        .float("scale_random", "Scale Random", 0.2, 0.0, 1.0)
        .float("rotation_random", "Rotation Random", 0.0, 0.0, 4.0)
        .float("missing_tiles", "Missing Tiles", 0.0, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .named_gray_out("mask", "Mortar Mask")
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let x_count = context.i("x_amount").max(1);
    let y_count = context.i("y_amount").max(1);
    let gap = context.f("gap").clamp(0.0, 0.49);
    let bevel = context.f("bevel").clamp(0.001, 0.5);
    let sc_rand = context.f("scale_random").clamp(0.0, 1.0);
    let rot_rand = context.f("rotation_random").clamp(0.0, 4.0);
    let missing = context.f("missing_tiles").clamp(0.0, 1.0);
    let lum_rand = context.f("luminance_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let n_x = x_count as f32;
    let n_y = y_count as f32;

    let mut height_data = vec![0.0f32; (w * h) as usize];
    let mut mask_data = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let cy = (v * n_y).floor() as i64;
        let v_cell = (v * n_y).fract();

        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let cx = (u * n_x).floor() as i64;
            let u_cell = (u * n_x).fract();

            let idx = (y * w + x) as usize;

            let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_mul(0x27d4_eb2d);
            let h1 = crate::random::hash(h0);
            let h2 = crate::random::hash(h1);
            let h3 = crate::random::hash(h2);
            let h4 = crate::random::hash(h3);

            let r_miss = h1 as f32 / 4_294_967_295.0;
            if r_miss < missing {
                // Missing tile
                height_data[idx] = 0.0;
                mask_data[idx] = 1.0;
                continue;
            }

            let r_sc = h2 as f32 / 4_294_967_295.0;
            let r_rot = h3 as f32 / 4_294_967_295.0;
            let r_lum = h4 as f32 / 4_294_967_295.0;

            // Rotate around tile center
            let angle = (r_rot * rot_rand).floor() * (PI * 0.5); // Stepped 90 deg or smooth
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            let lx = u_cell - 0.5;
            let ly = v_cell - 0.5;
            let rx = lx * cos_a - ly * sin_a;
            let ry = lx * sin_a + ly * cos_a;

            let eff_scale = (1.0 - r_sc * sc_rand * 0.4).max(0.1);
            let half_side = (0.5 - gap * 0.5) * eff_scale;

            let dx = rx.abs();
            let dy = ry.abs();

            if dx > half_side || dy > half_side {
                height_data[idx] = 0.0;
                mask_data[idx] = 1.0;
            } else {
                let bx = ((half_side - dx) / bevel).clamp(0.0, 1.0);
                let by = ((half_side - dy) / bevel).clamp(0.0, 1.0);
                let shape = bx.min(by);
                let profile = (shape * PI * 0.5).sin();
                let lum = 1.0 - r_lum * lum_rand;

                height_data[idx] = (profile * lum).clamp(0.0, 1.0);
                mask_data[idx] = 0.0;
            }
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(w, h, Kind::Gray, height_data, &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, mask_data, &limits).map_err(|e| e.to_string())?,
    ])
}
