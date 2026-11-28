use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const PATTERN_OPTIONS: &[&str] =
    &["Image Input", "Square", "Disc", "Paraboloid", "Bell", "Gaussian", "Thorn", "Pyramid", "Brick", "Gradation", "Waves", "Capsule", "Cone"];

pub const DISTRIBUTION_OPTIONS: &[&str] = &["Random", "By Pattern Number"];

pub(crate) fn definition() -> NodeDefinition {
    node("tile_sampler", "Tile Sampler", CATEGORY)
        .desc("Advanced pattern scatterer with multi-pattern image inputs and external driving maps for scale, rotation, displacement, color, and masking.")
        .any_in("pattern_input_1", "Pattern Input 1")
        .primary_input("pattern_input_1")
        .any_in("pattern_input_2", "Pattern Input 2")
        .any_in("pattern_input_3", "Pattern Input 3")
        .any_in("pattern_input_4", "Pattern Input 4")
        .any_in("pattern_input_5", "Pattern Input 5")
        .any_in("pattern_input_6", "Pattern Input 6")
        .gray_in("pattern_distribution_map", "Pattern Distribution Map")
        .any_in("background", "Background")
        .gray_in("scale_map", "Scale Map")
        .gray_in("rotation_map", "Rotation Map")
        .gray_in("displacement_map", "Displacement Map")
        .any_in("color_map", "Color Map")
        .gray_in("mask_map", "Mask Map")
        .int("x_amount", "X Amount", 4, 1, 64)
        .int("y_amount", "Y Amount", 4, 1, 64)
        .choice("pattern", "Pattern", PATTERN_OPTIONS, 1)
        .int("pattern_input_number", "Pattern Input Number", 1, 1, 6)
        .choice("pattern_distribution", "Pattern Distribution", DISTRIBUTION_OPTIONS, 0)
        .float("pattern_specific", "Pattern Specific", 0.0, 0.0, 1.0)
        .float("pattern_specific_random", "Pattern Specific Random", 0.0, 0.0, 1.0)
        .float("size_x", "Size X", 1.0, 0.0, 2.0)
        .float("size_y", "Size Y", 1.0, 0.0, 2.0)
        .float("scale", "Scale", 1.0, 0.0, 2.0)
        .float("scale_random", "Scale Random", 0.0, 0.0, 1.0)
        .float("scale_map_multiplier", "Scale Map Multiplier", 1.0, 0.0, 2.0)
        .float("offset", "Offset", 0.5, 0.0, 1.0)
        .float("position_random", "Position Random", 0.0, 0.0, 1.0)
        .float("displacement_map_multiplier", "Displacement Map Multiplier", 0.0, 0.0, 2.0)
        .angle("rotation", "Rotation", 0.0)
        .float("rotation_random", "Rotation Random", 0.0, 0.0, 1.0)
        .float("rotation_map_multiplier", "Rotation Map Multiplier", 1.0, 0.0, 2.0)
        .float("color_random", "Color Random", 0.0, 0.0, 1.0)
        .float("color_map_multiplier", "Color Map Multiplier", 1.0, 0.0, 2.0)
        .float("mask_map_threshold", "Mask Map Threshold", 0.0, 0.0, 1.0)
        .boolean("invert", "Invert", false)
        .int("seed", "Random Seed", 0, 0, 10000)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let pattern_input_number = context.i("pattern_input_number").clamp(1, 6) as usize;

    let (pattern_inputs, pattern_dist_map, background_input, scale_map, rotation_map, displacement_map, color_map, mask_map) =
        if context.inputs.len() >= 13 && context.inputs.len() > pattern_input_number + 6 {
            // Full definition inputs (6 pattern inputs + 7 map inputs)
            let pat_imgs: Vec<Option<&Image>> = (0..6).map(|i| context.input(i)).collect();
            (pat_imgs, context.sized(6), context.sized(7), context.sized(8), context.sized(9), context.sized(10), context.sized(11), context.sized(12))
        } else {
            // Filtered ports: pattern_input_number pattern inputs, followed by the 7 maps
            let pat_count = pattern_input_number.min(context.inputs.len());
            let pat_imgs: Vec<Option<&Image>> = (0..pat_count).map(|i| context.input(i)).collect();
            (
                pat_imgs,
                context.sized(pat_count),
                context.sized(pat_count + 1),
                context.sized(pat_count + 2),
                context.sized(pat_count + 3),
                context.sized(pat_count + 4),
                context.sized(pat_count + 5),
                context.sized(pat_count + 6),
            )
        };

    let x_amount = context.i("x_amount").clamp(1, 64) as usize;
    let y_amount = context.i("y_amount").clamp(1, 64) as usize;
    let pattern_choice = context.i("pattern");
    let pattern_distribution = context.i("pattern_distribution");
    let pattern_specific = context.f("pattern_specific").clamp(0.0, 1.0);
    let pattern_specific_random = context.f("pattern_specific_random").clamp(0.0, 1.0);
    let size_x = context.f("size_x").clamp(0.0, 2.0);
    let size_y = context.f("size_y").clamp(0.0, 2.0);
    let global_scale = context.f("scale").clamp(0.0, 2.0);
    let scale_random = context.f("scale_random").clamp(0.0, 1.0);
    let scale_map_multiplier = context.f("scale_map_multiplier").clamp(0.0, 2.0);
    let offset = context.f("offset");
    let position_random = context.f("position_random").clamp(0.0, 1.0);
    let displacement_map_multiplier = context.f("displacement_map_multiplier").clamp(0.0, 2.0);
    let base_rotation = context.f("rotation") * PI;
    let rotation_random = context.f("rotation_random").clamp(0.0, 1.0);
    let rotation_map_multiplier = context.f("rotation_map_multiplier").clamp(0.0, 2.0);
    let color_random = context.f("color_random").clamp(0.0, 1.0);
    let color_map_multiplier = context.f("color_map_multiplier").clamp(0.0, 2.0);
    let mask_map_threshold = context.f("mask_map_threshold").clamp(0.0, 1.0);
    let invert = context.b("invert");
    let user_seed = context.i("seed").max(0) as u32;

    let available_pattern_images: Vec<&Image> = pattern_inputs.iter().filter_map(|&opt| opt).collect();
    let has_image = !available_pattern_images.is_empty() && pattern_choice == 0;

    let any_pattern_color = available_pattern_images.iter().any(|img| img.kind() == Kind::Color);
    let kind = if has_image {
        if any_pattern_color { Kind::Color } else { Kind::Gray }
    } else if let Some(bg) = &background_input {
        bg.kind()
    } else if let Some(cm) = &color_map {
        cm.kind()
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
    let search_r = ((max_eff_s * 1.5 + position_random + displacement_map_multiplier).ceil() as i64).clamp(1, 4);

    Ok(vec![Image::generate_uv(context.w, context.h, kind, |u, v| {
        let mut best_pixel = [0.0f32; 4];
        let mut hit = false;
        let mut best_cov = -1.0f32;

        let base_cy = (v * n_y).floor() as i64;

        for dy in -search_r..=search_r {
            let cy_raw = base_cy + dy;
            let cy = cy_raw.rem_euclid(y_amount as i64) as usize;

            let row_offset = (cy % 2) as f32 * offset;
            let base_cx = ((u - row_offset * inv_nx) * n_x).floor() as i64;

            for dx in -search_r..=search_r {
                let cx_raw = base_cx + dx;
                let cx = cx_raw.rem_euclid(x_amount as i64) as usize;

                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ user_seed.wrapping_mul(0x27d4_eb2d);
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);
                let h3 = crate::random::hash(h2);
                let h4 = crate::random::hash(h3);
                let h5 = crate::random::hash(h4);
                let h6 = crate::random::hash(h5);
                let h7 = crate::random::hash(h6);

                let r_j1 = (h1 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_j2 = (h2 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_sc = h3 as f32 / 4_294_967_295.0;
                let r_rot = (h4 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
                let r_lum = h5 as f32 / 4_294_967_295.0;
                let r_ps = (h6 as f32 / 4_294_967_295.0) * 2.0 - 1.0;

                let center_u = (cx_raw as f32 + 0.5 + row_offset) * inv_nx;
                let center_v = (cy_raw as f32 + 0.5) * inv_ny;
                let norm_cu = center_u.rem_euclid(1.0);
                let norm_cv = center_v.rem_euclid(1.0);

                // Mask map culling
                if let Some(mask) = &mask_map {
                    let mask_sample = mask.sample_axes(norm_cu, norm_cv, tiling)[0];
                    if mask_sample < mask_map_threshold {
                        continue;
                    }
                }

                // Toroidal displacement
                let mut du = ((u - center_u + 0.5).rem_euclid(1.0)) - 0.5;
                let mut dv = ((v - center_v + 0.5).rem_euclid(1.0)) - 0.5;

                // Displacement map
                let (disp_u, disp_v) = if let Some(disp) = &displacement_map {
                    let disp_val = disp.sample_axes(norm_cu, norm_cv, tiling)[0];
                    (r_j1 * disp_val * displacement_map_multiplier, r_j2 * disp_val * displacement_map_multiplier)
                } else {
                    (0.0, 0.0)
                };

                du -= (r_j1 * position_random + disp_u) * 0.5 * inv_nx;
                dv -= (r_j2 * position_random + disp_v) * 0.5 * inv_ny;

                // Scale & scale map
                let sc_map_factor = if let Some(sc_map) = &scale_map {
                    let sc_val = sc_map.sample_axes(norm_cu, norm_cv, tiling)[0];
                    (1.0 - (1.0 - sc_val) * scale_map_multiplier).clamp(0.0, 2.0)
                } else {
                    1.0
                };

                let sc_factor = (1.0 - r_sc * scale_random * 0.5).max(0.01) * sc_map_factor;
                if sc_factor <= 1e-4 {
                    continue;
                }
                let eff_sx = (size_x * global_scale * sc_factor).max(0.001);
                let eff_sy = (size_y * global_scale * sc_factor).max(0.001);

                // Local cell coordinates [-0.5, 0.5]
                let lx = du * n_x;
                let ly = dv * n_y;

                // Rotation & rotation map
                let rot_map_angle = if let Some(rot_map) = &rotation_map {
                    let rot_val = rot_map.sample_axes(norm_cu, norm_cv, tiling)[0];
                    rot_val * rotation_map_multiplier * 2.0 * PI
                } else {
                    0.0
                };
                let angle = base_rotation + r_rot * rotation_random * PI + rot_map_angle;
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                let rx = lx * cos_a + ly * sin_a;
                let ry = -lx * sin_a + ly * cos_a;

                // Normalized tile coordinate
                let tx = rx / eff_sx;
                let ty = ry / eff_sy;

                // Color tint & luminance
                let lum = (1.0 - r_lum * color_random).clamp(0.0, 1.0);
                let tile_tint = if let Some(cm) = &color_map {
                    let c = cm.sample_axes(norm_cu, norm_cv, tiling);
                    [
                        (lum * (1.0 - color_map_multiplier + c[0] * color_map_multiplier)).clamp(0.0, 1.0),
                        (lum * (1.0 - color_map_multiplier + c[1] * color_map_multiplier)).clamp(0.0, 1.0),
                        (lum * (1.0 - color_map_multiplier + c[2] * color_map_multiplier)).clamp(0.0, 1.0),
                        1.0,
                    ]
                } else {
                    [lum, lum, lum, 1.0]
                };

                let eff_ps = (pattern_specific + r_ps * pattern_specific_random).clamp(0.0, 1.0);

                if has_image {
                    let num_active = pattern_input_number.min(pattern_inputs.len()).max(1);
                    let selected_slot = if let Some(dist_map) = &pattern_dist_map {
                        let sample_dist = dist_map.sample_axes(norm_cu, norm_cv, tiling)[0];
                        ((sample_dist * num_active as f32).floor() as usize).min(num_active - 1)
                    } else if pattern_distribution == 1 {
                        // By Pattern Number
                        (cx + cy * x_amount) % num_active
                    } else {
                        // Random
                        (h7 as usize) % num_active
                    };

                    let src_img = pattern_inputs.get(selected_slot).copied().flatten().or_else(|| available_pattern_images.first().copied());
                    if let Some(src_img) = src_img {
                        let pat_u = tx + 0.5;
                        let pat_v = ty + 0.5;
                        if (0.0..=1.0).contains(&pat_u) && (0.0..=1.0).contains(&pat_v) {
                            let sample = src_img.sample_axes(pat_u, pat_v, tiling);
                            hit = true;
                            for c in 0..channels {
                                let val = (sample[c] * tile_tint[c.min(3)]).clamp(0.0, 1.0);
                                if val > best_pixel[c] {
                                    best_pixel[c] = val;
                                }
                            }
                        }
                    }
                } else if tx.abs() <= 0.5 && ty.abs() <= 0.5 {
                    let (shape_cov, shape_val) = match pattern_choice {
                        // Disc (with hole cutout controlled by pattern_specific)
                        2 => {
                            let r = (tx * tx + ty * ty).sqrt();
                            let inner_r = eff_ps * 0.45;
                            let inside = r >= inner_r && r <= 0.5;
                            (inside, if inside { 1.0 } else { 0.0 })
                        }
                        // Paraboloid
                        3 => {
                            let r2 = (tx * tx + ty * ty) * 4.0;
                            let p = 1.0 + eff_ps * 3.0;
                            let inside = r2 <= 1.0;
                            (inside, if inside { (1.0 - r2).max(0.0).powf(p) } else { 0.0 })
                        }
                        // Bell
                        4 => {
                            let r2 = (tx * tx + ty * ty) * 4.0;
                            let k = 2.0 + eff_ps * 6.0;
                            let inside = r2 <= 1.0;
                            (inside, if inside { (-r2 * k).exp() } else { 0.0 })
                        }
                        // Gaussian
                        5 => {
                            let r2 = (tx * tx + ty * ty) * 4.0;
                            let sigma = 0.5 - eff_ps * 0.35;
                            let inside = r2 <= 1.0;
                            (inside, if inside { (-r2 / (2.0 * sigma * sigma)).exp() } else { 0.0 })
                        }
                        // Thorn
                        6 => {
                            let exponent = 0.5 + eff_ps * 0.5;
                            let v = (1.0 - (tx.abs().powf(exponent) + ty.abs().powf(exponent)) * 1.5).max(0.0);
                            (v > 0.0, v)
                        }
                        // Pyramid
                        7 => {
                            let d = tx.abs().max(ty.abs()) * 2.0;
                            let plateau = eff_ps * 0.8;
                            let v = if d <= plateau { 1.0 } else { (1.0 - (d - plateau) / (1.0 - plateau).max(0.001)).max(0.0) };
                            (v > 0.0, v)
                        }
                        // Brick
                        8 => {
                            let bevel = 0.05 + eff_ps * 0.4;
                            let bx = ((0.5 - tx.abs()) / bevel).clamp(0.0, 1.0);
                            let by = ((0.5 - ty.abs()) / bevel).clamp(0.0, 1.0);
                            let v = bx.min(by);
                            (v > 0.0, v)
                        }
                        // Gradation
                        9 => {
                            let angle = eff_ps * PI * 2.0;
                            let v = (tx * angle.cos() + ty * angle.sin() + 0.5).clamp(0.0, 1.0);
                            (true, v)
                        }
                        // Waves
                        10 => {
                            let freq = 1.0 + eff_ps * 5.0;
                            let v = (tx * PI * 2.0 * freq).sin() * 0.5 + 0.5;
                            (true, v)
                        }
                        // Capsule
                        11 => {
                            let cap_r = 0.25 - eff_ps * 0.1;
                            let hx = (tx.abs() - 0.25).max(0.0);
                            let d = (hx * hx + ty * ty).sqrt();
                            let inside = d <= cap_r;
                            (inside, if inside { 1.0 } else { 0.0 })
                        }
                        // Cone
                        12 => {
                            let r = (tx * tx + ty * ty).sqrt() * 2.0;
                            let v = (1.0 - r).max(0.0);
                            (v > 0.0, v)
                        }
                        // Square (with corner rounding controlled by pattern_specific)
                        _ => {
                            if eff_ps > 0.0 {
                                let r = eff_ps * 0.45;
                                let qx = tx.abs() - (0.5 - r);
                                let qy = ty.abs() - (0.5 - r);
                                let d = qx.max(qy).min(0.0) + ((qx.max(0.0)).powi(2) + (qy.max(0.0)).powi(2)).sqrt() - r;
                                (d <= 0.0, if d <= 0.0 { 1.0 } else { 0.0 })
                            } else {
                                (true, 1.0)
                            }
                        }
                    };

                    if shape_cov {
                        hit = true;
                        let cov_metric = shape_val.max(0.001);
                        if cov_metric > best_cov {
                            best_cov = cov_metric;
                            best_pixel = [
                                (shape_val * tile_tint[0]).clamp(0.0, 1.0),
                                (shape_val * tile_tint[1]).clamp(0.0, 1.0),
                                (shape_val * tile_tint[2]).clamp(0.0, 1.0),
                                1.0,
                            ];
                        }
                    }
                }
            }
        }

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
