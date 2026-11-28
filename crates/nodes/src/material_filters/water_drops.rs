use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("water_drops", "Water Drops", CATEGORY)
        .desc("Procedural water droplets and condensation scatter with meniscus height and surface normal generation.")
        .gray_in("height", "Height")
        .primary_input("height")
        .gray_in("mask", "Mask")
        .float("density", "Density", 16.0, 1.0, 64.0)
        .float("size", "Size", 0.08, 0.01, 0.5)
        .float("size_random", "Size Random", 0.5, 0.0, 1.0)
        .float("softness", "Softness", 0.1, 0.0, 1.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .named_gray_out("mask", "Mask")
        .named_color_out("normal", "Normal")
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let bg_height = context.sized(0);
    let mask_input = context.sized(1);

    let density = context.f("density").clamp(1.0, 64.0);
    let base_size = context.f("size").clamp(0.01, 0.5);
    let size_random = context.f("size_random").clamp(0.0, 1.0);
    let user_seed = context.i("seed").max(0) as u32;
    let tiling = context.tiling_axes();

    let grid_size = density.round().max(1.0) as usize;
    let inv_grid = 1.0 / grid_size as f32;

    let mut h_data = vec![0.0f32; (context.w * context.h) as usize];
    let mut m_data = vec![0.0f32; (context.w * context.h) as usize];

    for y in 0..context.h {
        let v = (y as f32 + 0.5) / context.h as f32;
        let gy = (v * grid_size as f32).floor() as i64;

        for x in 0..context.w {
            let u = (x as f32 + 0.5) / context.w as f32;
            let gx = (u * grid_size as f32).floor() as i64;
            let idx = (y * context.w + x) as usize;

            let mut best_drop = 0.0f32;
            let mut hit_mask = 0.0f32;

            // Search neighbor cells
            for dy in -1..=1 {
                let cy_raw = gy + dy;
                let cy = cy_raw.rem_euclid(grid_size as i64) as usize;

                for dx in -1..=1 {
                    let cx_raw = gx + dx;
                    let cx = cx_raw.rem_euclid(grid_size as i64) as usize;

                    let h0 = (cx as u32).wrapping_mul(0x9e37_79b9) ^ (cy as u32).wrapping_mul(0x85eb_ca6b) ^ user_seed.wrapping_mul(0xc2b2_ae35);
                    let h1 = crate::random::hash(h0);
                    let h2 = crate::random::hash(h1);
                    let h3 = crate::random::hash(h2);

                    let jx = (h1 as f32 / 4_294_967_295.0) * 0.8 + 0.1;
                    let jy = (h2 as f32 / 4_294_967_295.0) * 0.8 + 0.1;
                    let r_size = (1.0 - (h3 as f32 / 4_294_967_295.0) * size_random).max(0.05);

                    let center_u = (cx_raw as f32 + jx) * inv_grid;
                    let center_v = (cy_raw as f32 + jy) * inv_grid;

                    if let Some(m) = &mask_input {
                        let mv = m.sample_axes(center_u.rem_euclid(1.0), center_v.rem_euclid(1.0), tiling)[0];
                        if mv < 0.2 {
                            continue;
                        }
                    }

                    let du = ((u - center_u + 0.5).rem_euclid(1.0)) - 0.5;
                    let dv = ((v - center_v + 0.5).rem_euclid(1.0)) - 0.5;

                    let dist = (du * du + dv * dv).sqrt();
                    let drop_radius = base_size * r_size * inv_grid * 2.0;

                    if dist < drop_radius && drop_radius > 1e-5 {
                        let t = dist / drop_radius;
                        // Surface dome profile with meniscus contact angle
                        let dome = (1.0 - t * t).sqrt();
                        if dome > best_drop {
                            best_drop = dome;
                            hit_mask = 1.0;
                        }
                    }
                }
            }

            let bg = if let Some(bg) = &bg_height { bg.sample_axes(u, v, tiling)[0] } else { 0.0 };

            let final_h = (bg + best_drop).clamp(0.0, 1.0);
            if let Some(target) = h_data.get_mut(idx) {
                *target = final_h;
            }
            if let Some(target) = m_data.get_mut(idx) {
                *target = hit_mask;
            }
        }
    }

    let limits = craft_texture::Limits::default();
    let out_height = Image::from_data(context.w, context.h, Kind::Gray, h_data, &limits).map_err(|e| e.to_string())?;
    let out_mask = Image::from_data(context.w, context.h, Kind::Gray, m_data, &limits).map_err(|e| e.to_string())?;
    let normal = crate::atomic::height_to_normal(&out_height, 2.0, false, context.tile, false);
    Ok(vec![out_height, out_mask, normal])
}
