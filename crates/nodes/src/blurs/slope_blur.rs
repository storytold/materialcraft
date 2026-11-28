use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node, ops};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("slope_blur", "Slope Blur Grayscale", CATEGORY)
        .desc("Blurs or chips an image along the slope / gradient of a height map with optional masking.")
        .any_in("input", "Input")
        .primary_input("input")
        .gray_in("slope", "Slope Map")
        .gray_in("mask", "Mask")
        .float("intensity", "Intensity", 8.0, 0.0, 16.0)
        .int("samples", "Samples", 16, 1, 32)
        .choice("mode", "Mode", &["Blur", "Min", "Max"], 0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let slope_img = context.gray_or(1, 0.0);
    let mask_img = context.sized(2);

    let intensity = context.f("intensity").max(0.0);
    let samples = context.i("samples").clamp(1, 64);
    let mode = context.i("mode");

    const BASE_RESOLUTION: f32 = 256.0;
    let radius = intensity * (context.w.min(context.h) as f32 / BASE_RESOLUTION);
    let tiling = context.tiling_axes();
    let tile = tiling[0] && tiling[1];

    if radius <= 0.0 || samples <= 1 {
        return Ok(vec![input]);
    }

    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;
    let step = radius / samples as f32;
    let channels = input.channels().clamp(1, 4);

    Ok(vec![Image::generate(context.w, context.h, input.kind(), |x, y| {
        let orig = input.get(x as i64, y as i64, tile);

        let m = mask_img.as_ref().map_or(1.0, |m_img| m_img.get_gray(x as i64, y as i64, tile).clamp(0.0, 1.0));
        if m <= 1e-4 {
            return orig;
        }

        let (dx, dy) = ops::slope(&slope_img, x as i64, y as i64, tile);
        let len = (dx * dx + dy * dy).sqrt();

        if len < 1e-6 {
            return orig;
        }

        let dir_x = dx / len;
        let dir_y = dy / len;

        let mut accum = [0.0f32; 4];
        let mut min_val = [f32::MAX; 4];
        let mut max_val = [f32::MIN; 4];

        for s in 0..samples {
            let offset = s as f32 * step;
            let u = (x as f32 + 0.5 + dir_x * offset) * inv_w;
            let v = (y as f32 + 0.5 + dir_y * offset) * inv_h;

            let sample = input.sample_axes(u, v, tiling);
            for c in 0..channels {
                accum[c] += sample[c];
                if sample[c] < min_val[c] {
                    min_val[c] = sample[c];
                }
                if sample[c] > max_val[c] {
                    max_val[c] = sample[c];
                }
            }
        }

        let mut out = [0.0f32; 4];
        match mode {
            // Blur (Average)
            0 => {
                let inv_s = 1.0 / samples as f32;
                for c in 0..channels {
                    out[c] = accum[c] * inv_s;
                }
            }
            // Min (Chipping / Wear)
            1 => {
                out[..channels].copy_from_slice(&min_val[..channels]);
            }
            // Max
            2 => {
                out[..channels].copy_from_slice(&max_val[..channels]);
            }
            _ => {
                let inv_s = 1.0 / samples as f32;
                for c in 0..channels {
                    out[c] = accum[c] * inv_s;
                }
            }
        }

        // Apply mask blend
        if (m - 1.0).abs() > 1e-4 {
            for c in 0..channels {
                out[c] = orig[c] * (1.0 - m) + out[c] * m;
            }
        }

        if channels == 1 { [out[0], 0.0, 0.0, 0.0] } else { out }
    })])
}
