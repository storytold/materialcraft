use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("anisotropic_blur", "Anisotropic Blur", CATEGORY)
        .desc("Blurs an input image along directions defined by an angle/vector map.")
        .any_in("input", "Input")
        .primary_input("input")
        .gray_in("angle_map", "Angle Map")
        .float("intensity", "Intensity", 4.0, 0.0, 32.0)
        .int("samples", "Samples", 16, 4, 64)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let angle_map = context.sized(1);
    let intensity = context.f("intensity").clamp(0.0, 64.0);
    let samples = context.i("samples").clamp(4, 64) as usize;
    let tiling = context.tiling_axes();
    let kind = input.kind();

    if intensity <= 1e-4 {
        return Ok(vec![input]);
    }

    let inv_res = 1.0 / context.w.max(context.h) as f32;
    let half_samples = (samples / 2).max(1) as f32;
    let step_scale = intensity * inv_res / half_samples;

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let angle = angle_map.as_ref().map_or(0.0, |m| m.sample_axes(u, v, tiling)[0]) * PI * 2.0;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let mut accum = [0.0f32; 4];
        let mut total_w = 0.0f32;

        for s in 0..samples {
            let offset = (s as f32 - half_samples) * step_scale;
            let sample_u = u + cos_a * offset;
            let sample_v = v + sin_a * offset;
            let p = input.sample_axes(sample_u, sample_v, tiling);

            // Gaussian bell weighting along sample line
            let dist_norm = (s as f32 - half_samples) / half_samples;
            let weight = (-dist_norm * dist_norm * 2.0).exp();

            for c in 0..4 {
                accum[c] += p[c] * weight;
            }
            total_w += weight;
        }

        let inv_w = 1.0 / total_w.max(1e-5);
        [accum[0] * inv_w, accum[1] * inv_w, accum[2] * inv_w, accum[3] * inv_w]
    })])
}
