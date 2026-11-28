use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind, Limits};

pub(crate) fn definition() -> NodeDefinition {
    node("normal_to_height", "Normal to Height", CATEGORY)
        .desc("Reconstructs an estimated height displacement map from a tangent-space normal map via gradient integration.")
        .color_in("input", "Normal Map")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.05, 5.0)
        .int("iterations", "Iterations", 4, 1, 16)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w as usize, context.h as usize);
    let intensity = context.f("intensity").clamp(0.01, 10.0);
    let iterations = context.i("iterations").clamp(1, 16) as usize;
    let tiling = context.tiling_axes();

    // Extract gradients gx = -nx / nz, gy = -ny / nz
    let mut height = vec![0.5f32; w * h];
    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;

    for _iter in 0..iterations {
        for y in 0..context.h {
            let v = (y as f32 + 0.5) * inv_h;
            let y_i = y as usize;
            for x in 0..context.w {
                let u = (x as f32 + 0.5) * inv_w;
                let x_i = x as usize;
                let idx = y_i * w + x_i;

                let norm = input.sample_axes(u, v, tiling);
                let nx = norm[0] * 2.0 - 1.0;
                let ny = norm[1] * 2.0 - 1.0;
                let nz = (norm[2] * 2.0 - 1.0).max(0.05);

                let slope_x = (nx / nz) * intensity;
                let slope_y = (ny / nz) * intensity;

                let left_idx = y_i * w + (x_i + w - 1) % w;
                let right_idx = y_i * w + (x_i + 1) % w;
                let up_idx = ((y_i + h - 1) % h) * w + x_i;
                let down_idx = ((y_i + 1) % h) * w + x_i;

                let h_l = height[left_idx];
                let h_r = height[right_idx];
                let h_u = height[up_idx];
                let h_d = height[down_idx];

                // Poisson solver relaxation step: (h_l + h_r + h_u + h_d - (slope_x_diff + slope_y_diff)) / 4
                let avg = (h_l + h_r + h_u + h_d + (slope_x * 0.5) + (slope_y * 0.5)) * 0.25;
                height[idx] = avg;
            }
        }
    }

    // Normalize height range to [0, 1]
    let mut min_h = f32::MAX;
    let mut max_h = f32::MIN;
    for &val in &height {
        if val < min_h {
            min_h = val;
        }
        if val > max_h {
            max_h = val;
        }
    }

    let range = (max_h - min_h).max(1e-5);
    for val in &mut height {
        *val = ((*val - min_h) / range).clamp(0.0, 1.0);
    }

    let limits = Limits::default();
    Ok(vec![Image::from_data(context.w, context.h, Kind::Gray, height, &limits).map_err(|e| e.to_string())?])
}
