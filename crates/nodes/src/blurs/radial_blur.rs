use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("radial_blur", "Radial Blur", CATEGORY)
        .desc("Blurs pixels along radial paths radiating outward from or inward toward a central pivot point.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .int("samples", "Samples", 16, 4, 64)
        .float2("center", "Center", [0.5, 0.5], 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let intensity = context.f("intensity").clamp(0.0, 10.0);
    let samples = context.i("samples").clamp(4, 64) as usize;
    let center = context.v2("center");
    let tiling = context.tiling_axes();
    let kind = input.kind();

    if intensity <= 1e-4 {
        return Ok(vec![input]);
    }

    let inv_samples = 1.0 / samples as f32;
    let blur_factor = intensity * 0.05;

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let dx = u - center[0];
        let dy = v - center[1];

        let mut accum = [0.0f32; 4];
        for s in 0..samples {
            let t = (s as f32 * inv_samples) * blur_factor;
            let sample_u = u - dx * t;
            let sample_v = v - dy * t;
            let p = input.sample_axes(sample_u, sample_v, tiling);
            for c in 0..4 {
                accum[c] += p[c];
            }
        }

        [accum[0] * inv_samples, accum[1] * inv_samples, accum[2] * inv_samples, accum[3] * inv_samples]
    })])
}
