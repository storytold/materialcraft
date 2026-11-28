use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn channel_mixer_definition() -> NodeDefinition {
    node("channel_mixer", "Channel Mixer", CATEGORY)
        .desc("Matrix channel mixer allowing custom linear combinations of RGB channels.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("r_red", "Red -> Red", 1.0, -2.0, 2.0)
        .float("r_green", "Green -> Red", 0.0, -2.0, 2.0)
        .float("r_blue", "Blue -> Red", 0.0, -2.0, 2.0)
        .float("r_const", "Red Constant", 0.0, -1.0, 1.0)
        .float("g_red", "Red -> Green", 0.0, -2.0, 2.0)
        .float("g_green", "Green -> Green", 1.0, -2.0, 2.0)
        .float("g_blue", "Blue -> Green", 0.0, -2.0, 2.0)
        .float("g_const", "Green Constant", 0.0, -1.0, 1.0)
        .float("b_red", "Red -> Blue", 0.0, -2.0, 2.0)
        .float("b_green", "Green -> Blue", 0.0, -2.0, 2.0)
        .float("b_blue", "Blue -> Blue", 1.0, -2.0, 2.0)
        .float("b_const", "Blue Constant", 0.0, -1.0, 1.0)
        .color_out()
        .cook(cook_channel_mixer)
}

pub(crate) fn chrominance_extract_definition() -> NodeDefinition {
    node("chrominance_extract", "Chrominance Extract", CATEGORY)
        .desc("Extracts chrominance color channels (UV in YUV space) separated from luminance.")
        .color_in("input", "Input")
        .primary_input("input")
        .choice("mode", "Mode", &["UV (Color)", "U Channel (Grayscale)", "V Channel (Grayscale)"], 0)
        .color_out()
        .cook(cook_chrominance_extract)
}

pub(crate) fn color_match_definition() -> NodeDefinition {
    node("color_match", "Color Match", CATEGORY)
        .desc("Statistical color transfer matching mean and standard deviation of target to reference image.")
        .color_in("target", "Target Image")
        .primary_input("target")
        .color_in("reference", "Reference Palette")
        .float("strength", "Strength", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_color_match)
}

pub(crate) fn color_to_mask_definition() -> NodeDefinition {
    node("color_to_mask", "Color To Mask", CATEGORY)
        .desc("Generates a binary or feathered mask isolating a specific target color within tolerance.")
        .color_in("input", "Input")
        .primary_input("input")
        .color("color", "Target Color", [1.0, 0.0, 0.0, 1.0])
        .float("tolerance", "Tolerance", 0.2, 0.0, 1.0)
        .float("softness", "Softness", 0.1, 0.0, 1.0)
        .gray_out()
        .cook(cook_color_to_mask)
}

pub(crate) fn convert_to_linear_definition() -> NodeDefinition {
    node("convert_to_linear", "Convert To Linear", CATEGORY)
        .desc("Converts color from standard gamma sRGB to linear photometric color space.")
        .color_in("input", "Input")
        .primary_input("input")
        .color_out()
        .cook(cook_convert_to_linear)
}

pub(crate) fn convert_to_srgb_definition() -> NodeDefinition {
    node("convert_to_srgb", "Convert to sRGB", CATEGORY)
        .desc("Converts color from linear color space to standard sRGB gamma-encoded space.")
        .color_in("input", "Input")
        .primary_input("input")
        .color_out()
        .cook(cook_convert_to_srgb)
}

pub(crate) fn grayscale_conversion_advanced_definition() -> NodeDefinition {
    node("grayscale_conversion_advanced", "Grayscale Conversion Advanced", CATEGORY)
        .desc("Converts color images to grayscale using custom or standard photometric coefficients.")
        .color_in("input", "Input")
        .primary_input("input")
        .choice("preset", "Standard", &["Custom", "Rec. 709 (HDTV)", "Rec. 601 (SDTV)", "Maximum (Value)", "Average"], 1)
        .float("weight_r", "Red Weight", 0.2126, 0.0, 1.0)
        .float("weight_g", "Green Weight", 0.7152, 0.0, 1.0)
        .float("weight_b", "Blue Weight", 0.0722, 0.0, 1.0)
        .gray_out()
        .cook(cook_grayscale_advanced)
}

pub(crate) fn hald_clut_definition() -> NodeDefinition {
    node("hald_clut", "Hald CLUT", CATEGORY)
        .desc("Color grading lookup table applicator supporting standard Hald CLUT reference images.")
        .color_in("input", "Input")
        .primary_input("input")
        .color_in("clut", "Hald CLUT Image")
        .float("intensity", "Intensity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_hald_clut)
}

pub(crate) fn color_temperature_adj_definition() -> NodeDefinition {
    node("color_temperature_adjustment", "Color Temperature Adjustment", CATEGORY)
        .desc("White balance temperature and tint adjustment simulating Blackbody Kelvin spectrums.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("temperature", "Temperature (Kelvin)", 6500.0, 2000.0, 12000.0)
        .float("tint", "Tint (Magenta/Green)", 0.0, -1.0, 1.0)
        .color_out()
        .cook(cook_color_temp_adj)
}

pub(crate) fn replace_color_range_definition() -> NodeDefinition {
    node("replace_color_range", "Replace Color Range", CATEGORY)
        .desc("Replaces all colors falling within an RGB/HSL tolerance ball with a target replacement color.")
        .color_in("input", "Input")
        .primary_input("input")
        .color("source_color", "Source Color", [1.0, 0.0, 0.0, 1.0])
        .color("target_color", "Target Color", [0.0, 1.0, 0.0, 1.0])
        .float("range", "Color Range", 0.2, 0.0, 1.0)
        .float("feather", "Feathering", 0.1, 0.0, 1.0)
        .color_out()
        .cook(cook_replace_color_range)
}

fn cook_channel_mixer(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let rr = context.f("r_red");
    let rg = context.f("r_green");
    let rb = context.f("r_blue");
    let rc = context.f("r_const");

    let gr = context.f("g_red");
    let gg = context.f("g_green");
    let gb = context.f("g_blue");
    let gc = context.f("g_const");

    let br = context.f("b_red");
    let bg = context.f("b_green");
    let bb = context.f("b_blue");
    let bc = context.f("b_const");

    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let r = (p[0] * rr + p[1] * rg + p[2] * rb + rc).clamp(0.0, 1.0);
        let g = (p[0] * gr + p[1] * gg + p[2] * gb + gc).clamp(0.0, 1.0);
        let b = (p[0] * br + p[1] * bg + p[2] * bb + bc).clamp(0.0, 1.0);
        [r, g, b, p[3]]
    })])
}

fn cook_chrominance_extract(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let y = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        let u_val = (0.492 * (p[2] - y) + 0.5).clamp(0.0, 1.0);
        let v_val = (0.877 * (p[0] - y) + 0.5).clamp(0.0, 1.0);

        match mode {
            1 => [u_val, u_val, u_val, p[3]],
            2 => [v_val, v_val, v_val, p[3]],
            _ => [u_val, v_val, 0.5, p[3]],
        }
    })])
}

fn cook_color_match(context: &CookContext<'_>) -> CookResult {
    let target = context.any_or(0, Kind::Color, [0.0; 4]);
    let reference = context.any_or(1, Kind::Color, [1.0; 4]);
    let strength = context.f("strength").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    // Compute means and standard deviations
    let mut tgt_mean = [0.0f32; 3];
    let mut ref_mean = [0.0f32; 3];
    let n = (context.w * context.h) as f32;

    for y in 0..context.h {
        for x in 0..context.w {
            let u = (x as f32 + 0.5) / context.w as f32;
            let v = (y as f32 + 0.5) / context.h as f32;
            let t = target.sample_axes(u, v, tiling);
            let r = reference.sample_axes(u, v, tiling);
            tgt_mean[0] += t[0];
            tgt_mean[1] += t[1];
            tgt_mean[2] += t[2];
            ref_mean[0] += r[0];
            ref_mean[1] += r[1];
            ref_mean[2] += r[2];
        }
    }
    tgt_mean[0] /= n;
    tgt_mean[1] /= n;
    tgt_mean[2] /= n;
    ref_mean[0] /= n;
    ref_mean[1] /= n;
    ref_mean[2] /= n;

    let mut tgt_std = [0.0f32; 3];
    let mut ref_std = [0.0f32; 3];

    for y in 0..context.h {
        for x in 0..context.w {
            let u = (x as f32 + 0.5) / context.w as f32;
            let v = (y as f32 + 0.5) / context.h as f32;
            let t = target.sample_axes(u, v, tiling);
            let r = reference.sample_axes(u, v, tiling);
            for c in 0..3 {
                tgt_std[c] += (t[c] - tgt_mean[c]).powi(2);
                ref_std[c] += (r[c] - ref_mean[c]).powi(2);
            }
        }
    }
    for c in 0..3 {
        tgt_std[c] = (tgt_std[c] / n).sqrt().max(1e-4);
        ref_std[c] = (ref_std[c] / n).sqrt().max(1e-4);
    }

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = target.sample_axes(u, v, tiling);
        let mut out = [0.0f32; 4];
        for c in 0..3 {
            let transferred = ((p[c] - tgt_mean[c]) / tgt_std[c]) * ref_std[c] + ref_mean[c];
            out[c] = (p[c] * (1.0 - strength) + transferred * strength).clamp(0.0, 1.0);
        }
        out[3] = p[3];
        out
    })])
}

fn cook_color_to_mask(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let target = context.v4("color");
    let tol = context.f("tolerance").clamp(0.0, 2.0);
    let soft = context.f("softness").clamp(0.001, 1.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let dist = ((p[0] - target[0]).powi(2) + (p[1] - target[1]).powi(2) + (p[2] - target[2]).powi(2)).sqrt();
        let val = if dist <= tol {
            1.0
        } else if dist <= tol + soft {
            1.0 - (dist - tol) / soft
        } else {
            0.0
        };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_convert_to_linear(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let tiling = context.tiling_axes();

    let to_linear = |c: f32| -> f32 { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } };

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        [to_linear(p[0]).clamp(0.0, 1.0), to_linear(p[1]).clamp(0.0, 1.0), to_linear(p[2]).clamp(0.0, 1.0), p[3]]
    })])
}

fn cook_convert_to_srgb(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let tiling = context.tiling_axes();

    let to_srgb = |c: f32| -> f32 { if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 } };

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        [to_srgb(p[0]).clamp(0.0, 1.0), to_srgb(p[1]).clamp(0.0, 1.0), to_srgb(p[2]).clamp(0.0, 1.0), p[3]]
    })])
}

fn cook_grayscale_advanced(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let preset = context.i("preset");
    let (wr, wg, wb) = match preset {
        // Rec 709
        1 => (0.2126, 0.7152, 0.0722),
        // Rec 601
        2 => (0.299, 0.587, 0.114),
        // Max value
        3 => (0.0, 0.0, 0.0),
        // Average
        4 => (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0),
        // Custom
        _ => (context.f("weight_r"), context.f("weight_g"), context.f("weight_b")),
    };

    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let val = if preset == 3 { p[0].max(p[1]).max(p[2]) } else { p[0] * wr + p[1] * wg + p[2] * wb };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_hald_clut(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let clut = context.any_or(1, Kind::Color, [0.0; 4]);
    let intensity = context.f("intensity").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let clut_w = clut.width() as f32;
    let clut_h = clut.height() as f32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        if clut_w <= 1.0 || clut_h <= 1.0 {
            return p;
        }

        let r = p[0].clamp(0.0, 1.0);
        let g = p[1].clamp(0.0, 1.0);
        let b = p[2].clamp(0.0, 1.0);

        // Map RGB to 2D Hald coordinates
        let size = (clut_w.sqrt().round() as u32).max(1);
        let b_slice = (b * (size as f32 - 1.0)).round() as u32;
        let slice_x = b_slice % size;
        let slice_y = b_slice / size;

        let clut_u = (slice_x as f32 + r) / size as f32;
        let clut_v = (slice_y as f32 + g) / size as f32;

        let graded = clut.sample_axes(clut_u, clut_v, [false, false]);
        let out_r = p[0] * (1.0 - intensity) + graded[0] * intensity;
        let out_g = p[1] * (1.0 - intensity) + graded[1] * intensity;
        let out_b = p[2] * (1.0 - intensity) + graded[2] * intensity;
        [out_r.clamp(0.0, 1.0), out_g.clamp(0.0, 1.0), out_b.clamp(0.0, 1.0), p[3]]
    })])
}

fn cook_color_temp_adj(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let temp = context.f("temperature").clamp(2000.0, 12000.0);
    let tint = context.f("tint").clamp(-1.0, 1.0);
    let tiling = context.tiling_axes();

    // Approximate Kelvin to RGB shift (relative to 6500K neutral)
    let t = (temp - 6500.0) / 5500.0;
    let r_factor = 1.0 - t * 0.3;
    let b_factor = 1.0 + t * 0.3;
    let g_factor = 1.0 + tint * 0.2;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        [(p[0] * r_factor).clamp(0.0, 1.0), (p[1] * g_factor).clamp(0.0, 1.0), (p[2] * b_factor).clamp(0.0, 1.0), p[3]]
    })])
}

fn cook_replace_color_range(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let source = context.v4("source_color");
    let target = context.v4("target_color");
    let range = context.f("range").clamp(0.001, 2.0);
    let feather = context.f("feather").clamp(0.001, 1.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let dist = ((p[0] - source[0]).powi(2) + (p[1] - source[1]).powi(2) + (p[2] - source[2]).powi(2)).sqrt();
        let blend = if dist <= range {
            1.0
        } else if dist <= range + feather {
            1.0 - (dist - range) / feather
        } else {
            0.0
        };

        [p[0] * (1.0 - blend) + target[0] * blend, p[1] * (1.0 - blend) + target[1] * blend, p[2] * (1.0 - blend) + target[2] * blend, p[3]]
    })])
}
