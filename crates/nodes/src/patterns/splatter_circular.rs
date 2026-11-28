use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("splatter_circular", "Splatter Circular", CATEGORY)
        .desc("Scatters procedural discs or input patterns in concentric rings around a center origin with radius, ring count, angle spread, and random variations.")
        .any_in("pattern_input", "Pattern Input")
        .primary_input("pattern_input")
        .gray_in("mask_map", "Mask Map")
        .int("rings", "Rings", 4, 1, 32)
        .int("pattern_amount", "Pattern Amount", 12, 1, 128)
        .float("radius", "Radius", 0.4, 0.05, 1.0)
        .float("radius_random", "Radius Random", 0.1, 0.0, 1.0)
        .float("pattern_size", "Pattern Size", 0.05, 0.005, 0.5)
        .float("size_random", "Size Random", 0.3, 0.0, 1.0)
        .float("angle", "Angle", 0.0, 0.0, 360.0)
        .float("angle_random", "Angle Random", 0.0, 0.0, 360.0)
        .float("luminance_random", "Luminance Random", 0.2, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn hash_u32(mut x: u32) -> u32 {
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}

fn hash_f32(seed: u32, i: u32, j: u32) -> f32 {
    let mut h = seed.wrapping_add(i.wrapping_mul(374761393)).wrapping_add(j.wrapping_mul(668265263));
    h = hash_u32(h);
    (h & 0x00ffffff) as f32 / 16777215.0
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let width = context.w;
    let height = context.h;
    let rings = context.i("rings").max(1);
    let amount_per_ring = context.i("pattern_amount").max(1);
    let base_radius = context.f("radius").clamp(0.01, 1.0);
    let radius_random = context.f("radius_random").clamp(0.0, 1.0);
    let pattern_size = context.f("pattern_size").clamp(0.001, 0.5);
    let size_random = context.f("size_random").clamp(0.0, 1.0);
    let base_angle = context.f("angle").to_radians();
    let angle_random = context.f("angle_random").to_radians();
    let lum_random = context.f("luminance_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let pattern_input = context.input(0);
    let mask_map = context.input(1);

    let is_color = pattern_input.is_some_and(|img| img.kind() == Kind::Color);
    let tiling = context.tiling_axes();
    let mut out_data = if is_color { vec![0.0f32; (width * height * 4) as usize] } else { vec![0.0f32; (width * height) as usize] };

    let center_x = 0.5f32;
    let center_y = 0.5f32;

    // Render from outer to inner rings or iterate each ring
    for ring_idx in 0..rings {
        let ring_t = (ring_idx as f32 + 1.0) / rings as f32; // 0..1 fraction of base_radius
        let ring_r = ring_t * base_radius;
        // Scale number of items per ring proportionally to circumference
        let count = ((amount_per_ring as f32 * ring_t).round() as i32).max(1);

        for item_idx in 0..count {
            let u_idx = (ring_idx * 1000 + item_idx) as u32;
            let rand_r = hash_f32(seed, u_idx, 1);
            let rand_ang = hash_f32(seed, u_idx, 2);
            let rand_sz = hash_f32(seed, u_idx, 3);
            let rand_rot = hash_f32(seed, u_idx, 4);
            let rand_lum = hash_f32(seed, u_idx, 5);

            let r = ring_r + (rand_r * 2.0 - 1.0) * (base_radius / rings as f32) * radius_random;
            let theta = (item_idx as f32 / count as f32) * 2.0 * PI + base_angle + (rand_ang * 2.0 - 1.0) * angle_random;

            let px = center_x + r * theta.cos();
            let py = center_y + r * theta.sin();

            // Check mask if present
            if let Some(mask) = mask_map {
                let mask_val = mask.sample_axes(px.rem_euclid(1.0), py.rem_euclid(1.0), tiling)[0];
                if mask_val < 0.05 {
                    continue;
                }
            }

            let sz = pattern_size * (1.0 - rand_sz * size_random);
            let half_sz = sz * 0.5;
            let rot = rand_rot * 2.0 * PI;
            let cos_r = rot.cos();
            let sin_r = rot.sin();

            let lum = 1.0 - rand_lum * lum_random;

            // Bounding box in pixel coords
            let min_x = (((px - half_sz) * width as f32).floor() as i32).max(0);
            let max_x = (((px + half_sz) * width as f32).ceil() as i32).min(width as i32 - 1);
            let min_y = (((py - half_sz) * height as f32).floor() as i32).max(0);
            let max_y = (((py + half_sz) * height as f32).ceil() as i32).min(height as i32 - 1);

            for y in min_y..=max_y {
                let v = (y as f32 + 0.5) / height as f32;
                let dy = v - py;
                for x in min_x..=max_x {
                    let u = (x as f32 + 0.5) / width as f32;
                    let dx = u - px;

                    // Rotate into local splat frame [-0.5, 0.5]
                    let lx = (dx * cos_r - dy * sin_r) / sz + 0.5;
                    let ly = (dx * sin_r + dy * cos_r) / sz + 0.5;

                    if lx < 0.0 || lx > 1.0 || ly < 0.0 || ly > 1.0 {
                        continue;
                    }

                    let p_idx = (y as usize * width as usize + x as usize) * if is_color { 4 } else { 1 };

                    if let Some(pat) = pattern_input {
                        let sample = pat.sample_axes(lx, ly, [false, false]);
                        if is_color {
                            let alpha = sample[3];
                            for c in 0..3 {
                                out_data[p_idx + c] = out_data[p_idx + c].max(sample[c] * lum * alpha);
                            }
                            out_data[p_idx + 3] = out_data[p_idx + 3].max(alpha);
                        } else {
                            out_data[p_idx] = out_data[p_idx].max(sample[0] * lum);
                        }
                    } else {
                        // Default disc pattern
                        let dist = ((lx - 0.5) * (lx - 0.5) + (ly - 0.5) * (ly - 0.5)).sqrt() * 2.0;
                        if dist <= 1.0 {
                            let alpha = (1.0 - dist).clamp(0.0, 1.0);
                            let val = lum * alpha;
                            out_data[p_idx] = out_data[p_idx].max(val);
                        }
                    }
                }
            }
        }
    }

    let limits = craft_texture::Limits::default();
    if is_color {
        Ok(vec![Image::from_data(width, height, Kind::Color, out_data, &limits).map_err(|e| e.to_string())?])
    } else {
        Ok(vec![Image::from_data(width, height, Kind::Gray, out_data, &limits).map_err(|e| e.to_string())?])
    }
}
