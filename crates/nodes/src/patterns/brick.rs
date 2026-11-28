use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("brick", "Brick", CATEGORY)
        .desc("Procedural brick and tile generator with customizable rows, bevel width, mortaring gaps, row offsets, and random luminance variation.")
        .int("x_amount", "X Amount", 4, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .float("gap_x", "Gap X", 0.05, 0.0, 0.5)
        .float("gap_y", "Gap Y", 0.05, 0.0, 0.5)
        .float("bevel", "Bevel", 0.1, 0.0, 0.5)
        .float("offset", "Row Offset", 0.5, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .named_gray_out("mask", "Mortar Mask")
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("brick_1", "Brick 1", CATEGORY)
        .desc("Standard running bond brick masonry pattern.")
        .int("x_amount", "X Amount", 4, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .float("gap_x", "Gap X", 0.05, 0.0, 0.5)
        .float("gap_y", "Gap Y", 0.05, 0.0, 0.5)
        .float("bevel", "Bevel", 0.1, 0.0, 0.5)
        .float("offset", "Row Offset", 0.5, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .named_gray_out("mask", "Mortar Mask")
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("brick_2", "Brick 2", CATEGORY)
        .desc("Flemish / stack bond brick masonry pattern with narrow mortar.")
        .int("x_amount", "X Amount", 6, 1, 64)
        .int("y_amount", "Y Amount", 12, 1, 64)
        .float("gap_x", "Gap X", 0.03, 0.0, 0.5)
        .float("gap_y", "Gap Y", 0.03, 0.0, 0.5)
        .float("bevel", "Bevel", 0.08, 0.0, 0.5)
        .float("offset", "Row Offset", 0.25, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.4, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .named_gray_out("mask", "Mortar Mask")
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let x_count = context.i("x_amount").max(1);
    let y_count = context.i("y_amount").max(1);
    let gap_x = context.f("gap_x").clamp(0.0, 0.49);
    let gap_y = context.f("gap_y").clamp(0.0, 0.49);
    let bevel = context.f("bevel").clamp(0.001, 0.5);
    let offset = context.f("offset");
    let lum_rand = context.f("luminance_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let n_x = x_count as f32;
    let n_y = y_count as f32;
    let inv_nx = 1.0 / n_x;
    let _inv_ny = 1.0 / n_y;

    let mut height_data = vec![0.0f32; (w * h) as usize];
    let mut mask_data = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let cy = (v * n_y).floor() as i64;
        let v_cell = (v * n_y).fract();

        let row_offset = (cy.rem_euclid(2)) as f32 * offset;

        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let u_shifted = u - row_offset * inv_nx;
            let cx = (u_shifted * n_x).floor() as i64;
            let u_cell = (u_shifted * n_x).rem_euclid(1.0);

            let idx = (y * w + x) as usize;

            let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_mul(0x27d4_eb2d);
            let h1 = crate::random::hash(h0);
            let r_lum = h1 as f32 / 4_294_967_295.0;

            // Distance from brick center [-0.5, 0.5]
            let dx = (u_cell - 0.5).abs();
            let dy = (v_cell - 0.5).abs();

            let half_w = 0.5 - gap_x * 0.5;
            let half_h = 0.5 - gap_y * 0.5;

            if dx > half_w || dy > half_h {
                // In mortar
                height_data[idx] = 0.0;
                mask_data[idx] = 1.0;
            } else {
                let bx = ((half_w - dx) / bevel).clamp(0.0, 1.0);
                let by = ((half_h - dy) / bevel).clamp(0.0, 1.0);
                let shape = bx.min(by);

                // Smooth cosine bevel profile
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
