use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind, Limits};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("scratches", "Scratches Generator", CATEGORY)
        .desc("Generates procedural surface scratches with adjustable density, length, curvature, and width.")
        .int("density", "Density", 30, 1, 200)
        .float("length", "Length", 0.2, 0.02, 1.0)
        .float("length_variation", "Length Variation", 0.5, 0.0, 1.0)
        .float("width", "Width", 1.0, 0.25, 8.0)
        .angle("angle", "Base Angle", 0.0)
        .float("angle_variation", "Angle Variation", 1.0, 0.0, 1.0)
        .float("curviness", "Curviness", 0.2, 0.0, 1.0)
        .float("depth_variation", "Depth Variation", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let density = context.i("density").clamp(1, 200) as usize;
    let base_length = context.f("length").clamp(0.02, 1.0);
    let len_var = context.f("length_variation").clamp(0.0, 1.0);
    let width_param = context.f("width").clamp(0.25, 8.0);
    let base_angle = context.f("angle") * PI;
    let angle_var = context.f("angle_variation").clamp(0.0, 1.0);
    let curviness = context.f("curviness").clamp(0.0, 1.0);
    let depth_var = context.f("depth_variation").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let (w, h) = (context.w, context.h);
    let scale = (w as f32) / 256.0;
    let width_pixels = (width_param * scale).max(0.5);
    let total_pixels = (w as usize) * (h as usize);
    let mut data = vec![0.0f32; total_pixels];

    // Splat individual curved scratches
    for i in 0..density {
        let h0 = (i as u32).wrapping_mul(0x85eb_ca6b) ^ seed.wrapping_mul(0xc2b2_ae35);
        let h1 = crate::random::hash(h0);
        let h2 = crate::random::hash(h1);
        let h3 = crate::random::hash(h2);
        let h4 = crate::random::hash(h3);
        let h5 = crate::random::hash(h4);
        let h6 = crate::random::hash(h5);

        let p0_x = (h1 as f32 / 4_294_967_295.0) * w as f32;
        let p0_y = (h2 as f32 / 4_294_967_295.0) * h as f32;

        let r_len = 1.0 - (h3 as f32 / 4_294_967_295.0) * len_var;
        let scratch_len = base_length * r_len * w.max(h) as f32;

        let r_ang = (h4 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
        let ang = base_angle + r_ang * angle_var * PI;

        let r_curv = (h5 as f32 / 4_294_967_295.0) * 2.0 - 1.0;
        let curve_offset = r_curv * curviness * scratch_len * 0.3;

        let r_depth = 1.0 - (h6 as f32 / 4_294_967_295.0) * depth_var;
        let intensity = r_depth.clamp(0.1, 1.0);

        let dx = ang.cos() * scratch_len;
        let dy = ang.sin() * scratch_len;

        let p2_x = p0_x + dx;
        let p2_y = p0_y + dy;

        // Control point for quadratic Bezier
        let mid_x = (p0_x + p2_x) * 0.5;
        let mid_y = (p0_y + p2_y) * 0.5;
        let p1_x = mid_x - ang.sin() * curve_offset;
        let p1_y = mid_y + ang.cos() * curve_offset;

        // Bounding box of the entire curve + brush footprint
        let min_bx = p0_x.min(p1_x).min(p2_x) - width_pixels;
        let max_bx = p0_x.max(p1_x).max(p2_x) + width_pixels;
        let min_by = p0_y.min(p1_y).min(p2_y) - width_pixels;
        let max_by = p0_y.max(p1_y).max(p2_y) + width_pixels;

        // Number of sample segments along the scratch
        let num_steps = ((scratch_len / 2.0).ceil() as usize).max(4);
        let inv_steps = 1.0 / num_steps as f32;

        let mut pts = Vec::with_capacity(num_steps + 1);
        for s in 0..=num_steps {
            let t = s as f32 * inv_steps;
            let omt = 1.0 - t;
            let cur_x = omt * omt * p0_x + 2.0 * omt * t * p1_x + t * t * p2_x;
            let cur_y = omt * omt * p0_y + 2.0 * omt * t * p1_y + t * t * p2_y;
            pts.push((cur_x, cur_y));
        }

        let i_min_x = min_bx.floor() as i64;
        let i_max_x = max_bx.ceil() as i64;
        let i_min_y = min_by.floor() as i64;
        let i_max_y = max_by.ceil() as i64;

        for py in i_min_y..=i_max_y {
            let y_f = py as f32;
            let wrap_y = py.rem_euclid(h as i64) as usize;
            for px in i_min_x..=i_max_x {
                let x_f = px as f32;
                let wrap_x = px.rem_euclid(w as i64) as usize;

                // Distance to piecewise linear segment curve
                let mut min_d_sq = f32::MAX;
                for i in 0..num_steps {
                    let (ax, ay) = pts[i];
                    let (bx, by) = pts[i + 1];
                    let abx = bx - ax;
                    let aby = by - ay;
                    let apx = x_f - ax;
                    let apy = y_f - ay;
                    let seg_len_sq = (abx * abx + aby * aby).max(1e-6);
                    let t = (apx * abx + apy * aby) / seg_len_sq;
                    let t_clamped = t.clamp(0.0, 1.0);
                    let qx = ax + t_clamped * abx;
                    let qy = ay + t_clamped * aby;
                    let d_sq = (x_f - qx) * (x_f - qx) + (y_f - qy) * (y_f - qy);
                    if d_sq < min_d_sq {
                        min_d_sq = d_sq;
                    }
                }

                let d = min_d_sq.sqrt();
                if d <= width_pixels {
                    let brush = (1.0 - d / width_pixels).max(0.0) * intensity;
                    let idx = wrap_y * w as usize + wrap_x;
                    if let Some(target) = data.get_mut(idx)
                        && brush > *target
                    {
                        *target = brush;
                    }
                }
            }
        }
    }

    let result = Image::from_data(w, h, Kind::Gray, data, &Limits::default()).unwrap_or_else(|_| Image::filled(w, h, Kind::Gray, [0.0; 4]));
    Ok(vec![result])
}
