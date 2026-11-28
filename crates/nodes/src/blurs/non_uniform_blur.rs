use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn grayscale_definition() -> NodeDefinition {
    node("non_uniform_blur_grayscale", "Non Uniform Blur Grayscale", CATEGORY)
        .desc("Blurs a grayscale image with a per-pixel radius driven by a blur map, with optional anisotropy and direction.")
        .gray_in("input", "Input")
        .primary_input("input")
        .gray_in("blur_map", "Blur Map")
        .float("intensity", "Intensity", 16.0, 0.0, 64.0)
        .int("samples", "Samples", 16, 2, 32)
        .float("anisotropy", "Anisotropy", 0.0, -1.0, 1.0)
        .angle("angle", "Angle", 0.0)
        .gray_out()
        .cook(cook_grayscale)
}

pub(crate) fn color_definition() -> NodeDefinition {
    node("non_uniform_blur_color", "Non Uniform Blur Color", CATEGORY)
        .desc("Blurs a color image with a per-pixel radius driven by a blur map, with optional anisotropy and direction.")
        .color_in("input", "Input")
        .primary_input("input")
        .gray_in("blur_map", "Blur Map")
        .float("intensity", "Intensity", 16.0, 0.0, 64.0)
        .int("samples", "Samples", 16, 2, 32)
        .float("anisotropy", "Anisotropy", 0.0, -1.0, 1.0)
        .angle("angle", "Angle", 0.0)
        .color_out()
        .cook(cook_color)
}

fn cook_grayscale(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    cook_impl(context, &input, Kind::Gray)
}

fn cook_color(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    cook_impl(context, &input, Kind::Color)
}

fn cook_impl(context: &CookContext<'_>, input: &Image, kind: Kind) -> CookResult {
    let blur_map = context.sized(1);
    let intensity = context.f("intensity").clamp(0.0, 64.0);
    let num_samples = context.i("samples").clamp(2, 32) as usize;
    let anisotropy = context.f("anisotropy").clamp(-1.0, 1.0);
    let base_angle = context.f("angle") * PI;
    let tiling = context.tiling_axes();
    let channels = kind.channels();

    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;
    let inv_res = inv_w.min(inv_h);

    // Anisotropic scale along primary and secondary axes
    let scale_x = if anisotropy >= 0.0 { 1.0 } else { 1.0 + anisotropy };
    let scale_y = if anisotropy >= 0.0 { 1.0 - anisotropy } else { 1.0 };

    let cos_a = base_angle.cos();
    let sin_a = base_angle.sin();

    // Golden spiral sampling distribution
    let golden_angle = 2.399_963_2; // PI * (3.0 - 5.0.sqrt())

    Ok(vec![Image::generate_uv(context.w, context.h, kind, |u, v| {
        let b_val = if let Some(bm) = &blur_map { bm.sample_axes(u, v, tiling)[0].clamp(0.0, 1.0) } else { 1.0 };

        let radius = b_val * intensity * inv_res;
        if radius < 1e-5 {
            return input.sample_axes(u, v, tiling);
        }

        let mut accum = [0.0f32; 4];
        let mut total_w = 0.0f32;

        for s in 0..num_samples {
            let frac = (s as f32 + 0.5) / num_samples as f32;
            let r = frac.sqrt() * radius;
            let theta = s as f32 * golden_angle;

            let lx = theta.cos() * r * scale_x;
            let ly = theta.sin() * r * scale_y;

            let rx = lx * cos_a - ly * sin_a;
            let ry = lx * sin_a + ly * cos_a;

            let su = u + rx;
            let sv = v + ry;

            let weight = (-2.0 * frac * frac).exp();
            let sample = input.sample_axes(su, sv, tiling);

            for c in 0..channels {
                accum[c] += sample[c] * weight;
            }
            total_w += weight;
        }

        let inv_tw = 1.0 / total_w.max(1e-6);
        let mut out = [0.0f32; 4];
        for c in 0..channels {
            out[c] = (accum[c] * inv_tw).clamp(0.0, 1.0);
        }
        if channels == 4 {
            out[3] = input.sample_axes(u, v, tiling)[3];
        }
        out
    })])
}
