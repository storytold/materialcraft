use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const WEAVE_TYPE_OPTIONS: &[&str] = &["Plain (1x1)", "Twill (2x2)", "Basket (2x2)", "Satin (4x1)"];

pub(crate) fn definition() -> NodeDefinition {
    node("weave", "Weave", CATEGORY)
        .desc("Procedural fabric woven interlacing thread pattern with thread profile, curvature, gaps, and weave types.")
        .int("x_amount", "X Amount", 8, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .choice("weave_type", "Weave Type", WEAVE_TYPE_OPTIONS, 0)
        .float("thread_width", "Thread Width", 0.85, 0.1, 1.0)
        .float("thread_profile", "Thread Profile", 0.5, 0.0, 1.0)
        .float("gap_depth", "Gap Depth", 0.8, 0.0, 1.0)
        .float("thread_warp_curl", "Warp Curve", 0.5, 0.0, 1.0)
        .float("thread_weft_curl", "Weft Curve", 0.5, 0.0, 1.0)
        .int("thread_subdivision", "Thread Strands", 1, 1, 8)
        .gray_out()
        .named_gray_out("warp_mask", "Warp Mask")
        .named_gray_out("weft_mask", "Weft Mask")
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("weave_1", "Weave 1", CATEGORY)
        .desc("Plain 1x1 criss-cross over-under fabric weave.")
        .int("x_amount", "X Amount", 8, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .float("thread_width", "Thread Width", 0.85, 0.1, 1.0)
        .float("thread_profile", "Thread Profile", 0.5, 0.0, 1.0)
        .float("gap_depth", "Gap Depth", 0.8, 0.0, 1.0)
        .float("thread_warp_curl", "Warp Curve", 0.5, 0.0, 1.0)
        .float("thread_weft_curl", "Weft Curve", 0.5, 0.0, 1.0)
        .int("thread_subdivision", "Thread Strands", 1, 1, 8)
        .gray_out()
        .named_gray_out("warp_mask", "Warp Mask")
        .named_gray_out("weft_mask", "Weft Mask")
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("weave_2", "Weave 2", CATEGORY)
        .desc("Twill 2x2 diagonal ribbed denim/canvas weave.")
        .int("x_amount", "X Amount", 8, 1, 64)
        .int("y_amount", "Y Amount", 8, 1, 64)
        .choice("weave_type", "Weave Type", WEAVE_TYPE_OPTIONS, 1)
        .float("thread_width", "Thread Width", 0.9, 0.1, 1.0)
        .float("thread_profile", "Thread Profile", 0.5, 0.0, 1.0)
        .float("gap_depth", "Gap Depth", 0.8, 0.0, 1.0)
        .float("thread_warp_curl", "Warp Curve", 0.5, 0.0, 1.0)
        .float("thread_weft_curl", "Weft Curve", 0.5, 0.0, 1.0)
        .int("thread_subdivision", "Thread Strands", 1, 1, 8)
        .gray_out()
        .named_gray_out("warp_mask", "Warp Mask")
        .named_gray_out("weft_mask", "Weft Mask")
        .cook(cook)
}

fn is_warp_on_top(x_cell: i32, y_cell: i32, weave_type: usize) -> bool {
    match weave_type {
        // Plain 1x1: alternating checkerboard
        0 => (x_cell + y_cell).rem_euclid(2) == 0,
        // Twill 2x2: diagonal step
        1 => {
            let u = x_cell.rem_euclid(4);
            let v = y_cell.rem_euclid(4);
            (u + v).rem_euclid(4) < 2
        }
        // Basket 2x2: 2x2 blocks alternating
        2 => {
            let bx = (x_cell.div_euclid(2)).rem_euclid(2);
            let by = (y_cell.div_euclid(2)).rem_euclid(2);
            bx == by
        }
        // Satin 4x1
        _ => {
            let u = x_cell.rem_euclid(5);
            let v = y_cell.rem_euclid(5);
            (u + v * 2).rem_euclid(5) == 0
        }
    }
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let width = context.w as usize;
    let height = context.h as usize;
    let x_count = context.i("x_amount").max(1);
    let y_count = context.i("y_amount").max(1);
    let weave_type = context.i("weave_type").max(0) as usize;
    let thread_width = context.f("thread_width").clamp(0.01, 1.0);
    let thread_profile = context.f("thread_profile").clamp(0.0, 1.0);
    let gap_depth = context.f("gap_depth").clamp(0.0, 1.0);
    let strands = context.i("thread_subdivision").max(1) as f32;

    let mut out_data = vec![0.0f32; width * height];
    let mut warp_data = vec![0.0f32; width * height];
    let mut weft_data = vec![0.0f32; width * height];

    let w_inv = 1.0 / width as f32;
    let h_inv = 1.0 / height as f32;

    for y in 0..height {
        let v = (y as f32 + 0.5) * h_inv;
        let v_cell = (v * y_count as f32).floor() as i32;
        let v_local = (v * y_count as f32).fract(); // 0..1

        for x in 0..width {
            let u = (x as f32 + 0.5) * w_inv;
            let u_cell = (u * x_count as f32).floor() as i32;
            let u_local = (u * x_count as f32).fract(); // 0..1

            let idx = y * width + x;

            let warp_top = is_warp_on_top(u_cell, v_cell, weave_type);

            // Warp runs vertically (along y/v), spans across u
            // Weft runs horizontally (along x/u), spans across v
            let warp_dist = (u_local - 0.5).abs() * 2.0; // 0 at center, 1 at edge
            let weft_dist = (v_local - 0.5).abs() * 2.0; // 0 at center, 1 at edge

            // Thread cross-section rounded profile
            let warp_cross = if warp_dist < thread_width {
                let d = warp_dist / thread_width;
                (1.0 - d * d).max(0.0).powf(1.0 - thread_profile * 0.7)
            } else {
                0.0
            };

            let weft_cross = if weft_dist < thread_width {
                let d = weft_dist / thread_width;
                (1.0 - d * d).max(0.0).powf(1.0 - thread_profile * 0.7)
            } else {
                0.0
            };

            // Undulation along thread path (longitudinal curve)
            // When warp is on top, its crest is at the cell center (v_local = 0.5)
            let warp_wave = (v_local * PI).sin();
            let weft_wave = (u_local * PI).sin();

            // Multi-strand texture
            let warp_strands = if strands > 1.0 { ((u_local * strands * PI * 2.0).sin() * 0.5 + 0.5) * 0.2 + 0.8 } else { 1.0 };

            let weft_strands = if strands > 1.0 { ((v_local * strands * PI * 2.0).sin() * 0.5 + 0.5) * 0.2 + 0.8 } else { 1.0 };

            let warp_height = warp_cross * (0.5 + 0.5 * warp_wave) * warp_strands;
            let weft_height = weft_cross * (0.5 + 0.5 * weft_wave) * weft_strands;

            let (h, w_mask, f_mask) = if warp_top {
                // Warp is in front
                let base = if warp_cross > 0.05 {
                    warp_height
                } else if weft_cross > 0.05 {
                    weft_height * (1.0 - gap_depth * 0.5)
                } else {
                    0.0
                };
                (base, 1.0, 0.0)
            } else {
                // Weft is in front
                let base = if weft_cross > 0.05 {
                    weft_height
                } else if warp_cross > 0.05 {
                    warp_height * (1.0 - gap_depth * 0.5)
                } else {
                    0.0
                };
                (base, 0.0, 1.0)
            };

            out_data[idx] = h.clamp(0.0, 1.0);
            warp_data[idx] = if warp_cross > 0.1 { w_mask } else { 0.0 };
            weft_data[idx] = if weft_cross > 0.1 { f_mask } else { 0.0 };
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(context.w, context.h, Kind::Gray, out_data, &limits).map_err(|e| e.to_string())?,
        Image::from_data(context.w, context.h, Kind::Gray, warp_data, &limits).map_err(|e| e.to_string())?,
        Image::from_data(context.w, context.h, Kind::Gray, weft_data, &limits).map_err(|e| e.to_string())?,
    ])
}
