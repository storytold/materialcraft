use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn height_map_frequencies_mapper_definition() -> NodeDefinition {
    node("height_map_frequencies_mapper", "Height Map Frequencies Mapper", CATEGORY)
        .desc("Splits and rebalances low, medium, and high frequency surface details.")
        .gray_in("input", "Height Map")
        .primary_input("input")
        .float("low_frequency", "Low Frequency Gain", 1.0, 0.0, 2.0)
        .float("mid_frequency", "Mid Frequency Gain", 1.0, 0.0, 2.0)
        .float("high_frequency", "High Frequency Gain", 1.0, 0.0, 2.0)
        .gray_out()
        .cook(cook_frequencies_mapper)
}

pub(crate) fn lighting_cancel_high_definition() -> NodeDefinition {
    node("lighting_cancel_high_frequencies", "Lighting Cancel High Frequencies", CATEGORY)
        .desc("Removes sharp specular highlights and high-frequency directional lighting artifacts.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 2.0)
        .color_out()
        .cook(cook_lighting_cancel_high)
}

pub(crate) fn lighting_cancel_low_definition() -> NodeDefinition {
    node("lighting_cancel_low_frequencies", "Lighting Cancel Low Frequencies", CATEGORY)
        .desc("Removes macro gradients, uneven illumination, and vignetting across texture maps.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 2.0)
        .color_out()
        .cook(cook_lighting_cancel_low)
}

pub(crate) fn luminance_highpass_definition() -> NodeDefinition {
    node("luminance_highpass", "Luminance Highpass", CATEGORY)
        .desc("Extracts and enhances high-pass surface relief exclusively on the luminance channel.")
        .color_in("input", "Input")
        .primary_input("input")
        .float("radius", "Radius", 8.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.0, 0.1, 4.0)
        .color_out()
        .cook(cook_luminance_highpass)
}

fn cook_frequencies_mapper(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let low_gain = context.f("low_frequency").clamp(0.0, 4.0);
    let mid_gain = context.f("mid_frequency").clamp(0.0, 4.0);
    let high_gain = context.f("high_frequency").clamp(0.0, 4.0);

    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    // Box filter passes for fast frequency decomposition
    let blur_large = box_blur_gray(&input, w, h, 8, tiling);
    let blur_small = box_blur_gray(&input, w, h, 2, tiling);

    Ok(vec![Image::generate(w, h, Kind::Gray, |x, y| {
        let u = (x as f32 + 0.5) / w as f32;
        let v = (y as f32 + 0.5) / h as f32;

        let orig = input.sample_axes(u, v, tiling)[0];
        let idx = (y * w + x) as usize;
        let low = blur_large.get(idx).copied().unwrap_or(orig);
        let small = blur_small.get(idx).copied().unwrap_or(orig);

        let mid = small - low;
        let high = orig - small;

        let remapped = low * low_gain + mid * mid_gain + high * high_gain;
        [remapped.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_lighting_cancel_high(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let intensity = context.f("intensity").clamp(0.0, 4.0);
    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    let lowpass = box_blur_color(&input, w, h, 3, tiling);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let u = (x as f32 + 0.5) / w as f32;
        let v = (y as f32 + 0.5) / h as f32;
        let orig = input.sample_axes(u, v, tiling);
        let idx = (y * w + x) as usize;
        let lp = lowpass.get(idx).copied().unwrap_or(orig);

        let mut out = [0.0f32; 4];
        for c in 0..3 {
            let high = orig[c] - lp[c];
            out[c] = (orig[c] - high * intensity).clamp(0.0, 1.0);
        }
        out[3] = orig[3];
        out
    })])
}

fn cook_lighting_cancel_low(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let intensity = context.f("intensity").clamp(0.0, 4.0);
    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    let lowpass = box_blur_color(&input, w, h, 16, tiling);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let u = (x as f32 + 0.5) / w as f32;
        let v = (y as f32 + 0.5) / h as f32;
        let orig = input.sample_axes(u, v, tiling);
        let idx = (y * w + x) as usize;
        let lp = lowpass.get(idx).copied().unwrap_or(orig);

        let mut out = [0.0f32; 4];
        for c in 0..3 {
            let ratio = orig[c] / (lp[c].max(0.05));
            out[c] = (orig[c] * (1.0 - intensity * 0.5) + ratio * 0.5 * intensity).clamp(0.0, 1.0);
        }
        out[3] = orig[3];
        out
    })])
}

fn cook_luminance_highpass(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let radius = context.f("radius").clamp(1.0, 64.0).round() as usize;
    let contrast = context.f("contrast").clamp(0.1, 4.0);
    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    let lowpass = box_blur_color(&input, w, h, radius, tiling);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let u = (x as f32 + 0.5) / w as f32;
        let v = (y as f32 + 0.5) / h as f32;
        let orig = input.sample_axes(u, v, tiling);
        let idx = (y * w + x) as usize;
        let lp = lowpass.get(idx).copied().unwrap_or(orig);

        let orig_lum = 0.299 * orig[0] + 0.587 * orig[1] + 0.114 * orig[2];
        let lp_lum = 0.299 * lp[0] + 0.587 * lp[1] + 0.114 * lp[2];
        let hp_lum = (orig_lum - lp_lum) * contrast;

        [(orig[0] + hp_lum).clamp(0.0, 1.0), (orig[1] + hp_lum).clamp(0.0, 1.0), (orig[2] + hp_lum).clamp(0.0, 1.0), orig[3]]
    })])
}

fn box_blur_gray(img: &Image, w: u32, h: u32, r: usize, tiling: [bool; 2]) -> Vec<f32> {
    let mut out = vec![0.0f32; (w * h) as usize];
    let step_x = 1.0 / w as f32;
    let step_y = 1.0 / h as f32;
    let samples = (2 * r + 1) as f32;

    for y in 0..h {
        let v = (y as f32 + 0.5) * step_y;
        for x in 0..w {
            let u = (x as f32 + 0.5) * step_x;
            let mut sum = 0.0f32;
            for i in -(r as i32)..=(r as i32) {
                let s = img.sample_axes(u + (i as f32) * step_x, v, tiling)[0];
                sum += s;
            }
            out[(y * w + x) as usize] = sum / samples;
        }
    }
    out
}

fn box_blur_color(img: &Image, w: u32, h: u32, r: usize, tiling: [bool; 2]) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; (w * h) as usize];
    let step_x = 1.0 / w as f32;
    let step_y = 1.0 / h as f32;
    let samples = (2 * r + 1) as f32;

    for y in 0..h {
        let v = (y as f32 + 0.5) * step_y;
        for x in 0..w {
            let u = (x as f32 + 0.5) * step_x;
            let mut sum = [0.0f32; 4];
            for i in -(r as i32)..=(r as i32) {
                let s = img.sample_axes(u + (i as f32) * step_x, v, tiling);
                sum[0] += s[0];
                sum[1] += s[1];
                sum[2] += s[2];
                sum[3] += s[3];
            }
            out[(y * w + x) as usize] = [sum[0] / samples, sum[1] / samples, sum[2] / samples, sum[3] / samples];
        }
    }
    out
}
