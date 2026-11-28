use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const PATTERN_OPTIONS: &[&str] = &["Image Input", "Square", "Disc", "Paraboloid", "Bell", "Pyramid", "Brick", "Waves"];

pub(crate) fn definition() -> NodeDefinition {
    node("tile_generator", "Tile Generator", CATEGORY)
        .desc("Repeats an input pattern or geometric shape across a 2D grid with offset, position random, scale, rotation, and luminance random.")
        .any_in("pattern_input", "Pattern Input")
        .primary_input("pattern_input")
        .any_in("background", "Background")
        .int("x_amount", "X Amount", 4, 1, 64)
        .int("y_amount", "Y Amount", 4, 1, 64)
        .choice("pattern", "Pattern", PATTERN_OPTIONS, 1)
        .float("size_x", "Size X", 1.0, 0.0, 2.0)
        .float("size_y", "Size Y", 1.0, 0.0, 2.0)
        .float("scale", "Scale", 1.0, 0.0, 2.0)
        .float("scale_random", "Scale Random", 0.0, 0.0, 1.0)
        .float("offset", "Offset", 0.5, 0.0, 1.0)
        .float("position_random", "Position Random", 0.0, 0.0, 1.0)
        .angle("rotation", "Rotation", 0.0)
        .float("rotation_random", "Rotation Random", 0.0, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.0, 0.0, 1.0)
        .boolean("invert", "Invert", false)
        .int("seed", "Random Seed", 0, 0, 10000)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input_pattern = context.input(0);
    let background_input = context.sized(1);

    let x_amount = context.i("x_amount").clamp(1, 64) as usize;
    let y_amount = context.i("y_amount").clamp(1, 64) as usize;
    let pattern_choice = context.i("pattern");
    let size_x = context.f("size_x").clamp(0.0, 2.0);
    let size_y = context.f("size_y").clamp(0.0, 2.0);
    let global_scale = context.f("scale").clamp(0.0, 2.0);
    let scale_random = context.f("scale_random").clamp(0.0, 1.0);
    let offset = context.f("offset");
    let position_random = context.f("position_random").clamp(0.0, 1.0);
    let base_rotation = context.f("rotation") * PI;
    let rotation_random = context.f("rotation_random").clamp(0.0, 1.0);
    let luminance_random = context.f("luminance_random").clamp(0.0, 1.0);
    let invert = context.b("invert");
    let user_seed = context.i("seed").max(0) as u32;

    let has_image = input_pattern.is_some() && pattern_choice == 0;
    let kind = if has_image {
        input_pattern.map_or(Kind::Gray, |img| img.kind())
    } else if let Some(bg) = &background_input {
        bg.kind()
    } else {
        Kind::Gray
    };

    let channels = kind.channels();
    let n_x = x_amount as f32;
    let n_y = y_amount as f32;
    let inv_nx = 1.0 / n_x;
    let inv_ny = 1.0 / n_y;
    let tiling = context.tiling_axes();

    let max_eff_s = (size_x.max(size_y) * global_scale).max(0.5);
    let search_r = ((max_eff_s * 1.5 + position_random).ceil() as i64).clamp(1, 4);

    Ok(vec![Image::generate_uv(context.w, context.h, kind, |u, v| {
        let mut best_pixel = [0.0f32; 4];
        let mut hit = false;
        let mut best_val = -1.0f32;

        // Primary base row
        let base_cy = (v * n_y).floor() as i64;

        // Search candidate cells around current pixel to support jitter & overlap
        for dy in -search_r..=search_r {
            let cy_raw = base_cy + dy;
            let cy = cy_raw.rem_euclid(y_amount as i64) as usize;

            let row_offset = (cy % 2) as f32 * offset;
            let base_cx = ((u - row_offset * inv_nx) * n_x).floor() as i64;

            for dx in -search_r..=search_r {
                let cx_raw = base_cx + dx;
                let cx = cx_raw.rem_euclid(x_amount as i64) as usize;

                // Deterministic hash per cell
                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ user_seed.wrapping_mul(0x27d4_eb2d);
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);
                let h3 = crate::random::hash(h2);
                let h4 = crate::random::hash(h3);
                let h5 = crate::random::hash(h4);

                let r_j1 = (h1 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_j2 = (h2 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_sc = h3 as f32 / 4_294_967_295.0;
                let r_rot = (h4 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_lum = h5 as f32 / 4_294_967_295.0;

                let center_u = (cx_raw as f32 + 0.5 + row_offset) * inv_nx;
                let center_v = (cy_raw as f32 + 0.5) * inv_ny;

                // Toroidal displacement
                let mut du = ((u - center_u + 0.5).rem_euclid(1.0)) - 0.5;
                let mut dv = ((v - center_v + 0.5).rem_euclid(1.0)) - 0.5;

                // Position random
                du -= r_j1 * position_random * 0.5 * inv_nx;
                dv -= r_j2 * position_random * 0.5 * inv_ny;

                // Scale
                let sc_factor = (1.0 - r_sc * scale_random * 0.5).max(0.01);
                let eff_sx = (size_x * global_scale * sc_factor).max(0.001);
                let eff_sy = (size_y * global_scale * sc_factor).max(0.001);

                // Local cell coordinates [-0.5, 0.5]
                let lx = du * n_x;
                let ly = dv * n_y;

                // Rotation
                let angle = base_rotation + r_rot * rotation_random * PI;
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                let rx = lx * cos_a + ly * sin_a;
                let ry = -lx * sin_a + ly * cos_a;

                // Normalized tile coordinate
                let tx = rx / eff_sx;
                let ty = ry / eff_sy;

                let lum = (1.0 - r_lum * luminance_random).clamp(0.0, 1.0);

                if has_image {
                    if let Some(src_img) = input_pattern {
                        let pat_u = tx + 0.5;
                        let pat_v = ty + 0.5;
                        if (0.0..=1.0).contains(&pat_u) && (0.0..=1.0).contains(&pat_v) {
                            let sample = src_img.sample_axes(pat_u, pat_v, tiling);
                            hit = true;
                            for c in 0..channels {
                                let val = (sample[c] * lum).clamp(0.0, 1.0);
                                if val > best_pixel[c] {
                                    best_pixel[c] = val;
                                }
                            }
                        }
                    }
                } else if tx.abs() <= 0.5 && ty.abs() <= 0.5 {
                    let shape_cov = match pattern_choice {
                        // Disc
                        2 => {
                            let r = (tx * tx + ty * ty).sqrt();
                            r <= 0.5
                        }
                        _ => true,
                    };

                    if shape_cov {
                        hit = true;
                        let shape_val = match pattern_choice {
                            // Disc
                            2 => 1.0,
                            // Paraboloid
                            3 => (1.0 - 4.0 * (tx * tx + ty * ty)).max(0.0),
                            // Bell
                            4 => {
                                let r2 = 4.0 * (tx * tx + ty * ty);
                                (-r2 * 2.0).exp()
                            }
                            // Pyramid
                            5 => (1.0 - 2.0 * tx.abs().max(ty.abs())).max(0.0),
                            // Brick
                            6 => {
                                let bx = (0.5 - tx.abs()) * 10.0;
                                let by = (0.5 - ty.abs()) * 10.0;
                                bx.min(by).clamp(0.0, 1.0)
                            }
                            // Waves
                            7 => (tx * PI * 2.0).sin() * 0.5 + 0.5,
                            // Square (default)
                            _ => 1.0,
                        };

                        let val = (shape_val * lum).clamp(0.0, 1.0);
                        if val > best_val {
                            best_val = val;
                            best_pixel = [val, val, val, 1.0];
                        }
                    }
                }
            }
        }

        // Fill background if no tile hit and background input is connected
        let mut final_pixel =
            if !hit { if let Some(bg) = &background_input { bg.sample_axes(u, v, tiling) } else { [0.0, 0.0, 0.0, 1.0] } } else { best_pixel };

        if invert {
            for c in 0..channels.min(3) {
                final_pixel[c] = 1.0 - final_pixel[c];
            }
        }

        final_pixel
    })])
}
