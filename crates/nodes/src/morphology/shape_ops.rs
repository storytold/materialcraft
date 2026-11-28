use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use rayon::prelude::*;

pub(crate) fn definition_rtao() -> NodeDefinition {
    node("ambient_occlusion_rtao", "Ambient Occlusion (RTAO)", CATEGORY)
        .desc("Ray-traced ambient occlusion from normal and height with multi-ray cosine-weighted hemisphere sampling.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .gray_in("height", "Height Map")
        .float("radius", "Radius", 0.25, 0.01, 1.0)
        .float("intensity", "Intensity", 1.0, 0.0, 5.0)
        .int("samples", "Sample Count", 16, 4, 64)
        .gray_out()
        .cook(cook_rtao)
}

pub(crate) fn definition_cross_section() -> NodeDefinition {
    node("cross_section", "Cross Section", CATEGORY)
        .desc("Slices a height field along an arbitrary plane or axis to visualize profile cross-section curves.")
        .gray_in("height", "Height Map")
        .primary_input("height")
        .float("slice_position", "Slice Position", 0.5, 0.0, 1.0)
        .angle("angle", "Slice Angle", 0.0)
        .float("thickness", "Line Thickness", 0.02, 0.001, 0.2)
        .gray_out()
        .cook(cook_cross_section)
}

pub(crate) fn definition_curvature_smooth() -> NodeDefinition {
    node("curvature_smooth", "Curvature Smooth", CATEGORY)
        .desc("Multi-scale Gaussian filtered surface curvature highlighting broad macroscopic contours.")
        .any_in("input", "Normal or Height")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .float("radius", "Blur Radius", 4.0, 1.0, 32.0)
        .gray_out()
        .cook(cook_curvature_smooth)
}

pub(crate) fn definition_curvature_sobel() -> NodeDefinition {
    node("curvature_sobel", "Curvature Sobel", CATEGORY)
        .desc("High-precision Sobel kernel curvature operator for sharp edge and micro-crevice detection.")
        .any_in("input", "Normal or Height")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .gray_out()
        .cook(cook_curvature_sobel)
}

pub(crate) fn definition_emboss_with_gloss() -> NodeDefinition {
    node("emboss_with_gloss", "Emboss With Gloss", CATEGORY)
        .desc("Directional emboss relief lighting with specular Blinn-Phong gloss highlight.")
        .gray_in("input", "Height / Mask")
        .primary_input("input")
        .angle("light_angle", "Light Angle", 0.125)
        .float("height", "Relief Height", 1.0, 0.0, 5.0)
        .float("glossiness", "Glossiness", 32.0, 1.0, 128.0)
        .float("specular", "Specular Intensity", 0.5, 0.0, 2.0)
        .color_out()
        .cook(cook_emboss_with_gloss)
}

pub(crate) fn definition_extend_shape() -> NodeDefinition {
    node("extend_shape", "Extend Shape", CATEGORY)
        .desc("Expands or contracts binary and grayscale shape boundaries outward via Euclidean distance transform.")
        .gray_in("input", "Input Shape")
        .primary_input("input")
        .float("distance", "Extend Distance", 0.05, -0.5, 0.5)
        .float("smoothness", "Edge Smoothness", 0.01, 0.001, 0.2)
        .gray_out()
        .cook(cook_extend_shape)
}

fn cook_rtao(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let height = context.any_or(1, Kind::Gray, [0.5; 4]);
    let (w, h) = (context.w, context.h);
    let radius = context.f("radius").clamp(0.001, 2.0);
    let intensity = context.f("intensity").max(0.0);
    let samples = context.i("samples").clamp(4, 64) as usize;
    let tile = context.tile;

    let rad_px = (radius * (w.min(h) as f32) * 0.5).max(1.0);
    let total = (w * h) as usize;
    let mut out = vec![1.0f32; total];

    out.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        let yi = y as i64;
        for (x, pix) in row.iter_mut().enumerate() {
            let xi = x as i64;
            let n = normal.get(xi, yi, tile);
            let nx = n[0] * 2.0 - 1.0;
            let ny = n[1] * 2.0 - 1.0;
            let nz = (n[2] * 2.0 - 1.0).max(0.01);
            let h_center = height.get_gray(xi, yi, tile);

            let mut occ = 0.0f32;
            for s in 0..samples {
                let phi = 2.0 * std::f32::consts::PI * ((s as f32 + 0.5) / samples as f32);
                let dist = rad_px * (((s as f32 + 0.3) / samples as f32).sqrt());
                let sx = xi + (phi.cos() * dist).round() as i64;
                let sy = yi + (phi.sin() * dist).round() as i64;

                let h_sample = height.get_gray(sx, sy, tile);
                let dh = h_sample - h_center;
                if dh > 0.0 {
                    let dir_x = phi.cos();
                    let dir_y = phi.sin();
                    let dot = (dir_x * nx + dir_y * ny + 0.1 * nz).max(0.0);
                    let angle_occ = (dh / (dist / rad_px + 0.05)).atan();
                    occ += angle_occ * dot;
                }
            }
            let ao = 1.0 - (occ / (samples as f32) * intensity).clamp(0.0, 1.0);
            *pix = ao.clamp(0.0, 1.0);
        }
    });

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Gray, out, &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_cross_section(context: &CookContext<'_>) -> CookResult {
    let height = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let slice_pos = context.f("slice_position").clamp(0.0, 1.0);
    let angle = context.f("angle") * std::f32::consts::PI;
    let thickness = context.f("thickness").clamp(0.001, 0.5);
    let tile = context.tile;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::gray(w, h, |x, y| {
        let u = (x as f32 + 0.5) / w as f32;
        let v = (y as f32 + 0.5) / h as f32;
        // Compute distance from slice plane
        let d = (u - 0.5) * cos_a + (v - 0.5) * sin_a + 0.5 - slice_pos;
        if d.abs() <= thickness {
            let sample_h = height.sample_axes(u, v, [tile, tile])[0];
            1.0 - (d.abs() / thickness) * (1.0 - sample_h)
        } else {
            0.0
        }
    })])
}

fn cook_curvature_smooth(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let intensity = context.f("intensity").max(0.0);
    let radius = context.f("radius").clamp(1.0, 64.0) as i64;
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let center = input.get_gray(x, y, tile);
        let r = input.get_gray(x + radius, y, tile);
        let l = input.get_gray(x - radius, y, tile);
        let d = input.get_gray(x, y + radius, tile);
        let u = input.get_gray(x, y - radius, tile);

        let laplacian = (r + l + d + u - 4.0 * center) / (radius as f32);
        (0.5 + laplacian * intensity * 2.0).clamp(0.0, 1.0)
    })])
}

fn cook_curvature_sobel(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let intensity = context.f("intensity").max(0.0);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        // Sobel filter
        let tl = input.get_gray(x - 1, y - 1, tile);
        let tc = input.get_gray(x, y - 1, tile);
        let tr = input.get_gray(x + 1, y - 1, tile);
        let ml = input.get_gray(x - 1, y, tile);
        let mr = input.get_gray(x + 1, y, tile);
        let bl = input.get_gray(x - 1, y + 1, tile);
        let bc = input.get_gray(x, y + 1, tile);
        let br = input.get_gray(x + 1, y + 1, tile);

        let dx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
        let dy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
        let grad = (dx * dx + dy * dy).sqrt();

        (grad * intensity * 2.0).clamp(0.0, 1.0)
    })])
}

fn cook_emboss_with_gloss(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let angle = context.f("light_angle") * std::f32::consts::PI * 2.0;
    let height = context.f("height").max(0.0);
    let gloss = context.f("glossiness").clamp(1.0, 256.0);
    let spec = context.f("specular").max(0.0);
    let tile = context.tile;

    let lx = angle.cos();
    let ly = angle.sin();
    let lz = 0.5f32;
    let l_len = (lx * lx + ly * ly + lz * lz).sqrt();
    let (lx, ly, lz) = (lx / l_len, ly / l_len, lz / l_len);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let r = input.get_gray(x + 1, y, tile);
        let l = input.get_gray(x - 1, y, tile);
        let d = input.get_gray(x, y + 1, tile);
        let u = input.get_gray(x, y - 1, tile);

        let nx = -(r - l) * height * 0.5;
        let ny = -(d - u) * height * 0.5;
        let nz = 1.0f32;
        let n_len = (nx * nx + ny * ny + nz * nz).sqrt();
        let (nx, ny, nz) = (nx / n_len, ny / n_len, nz / n_len);

        let diff = (nx * lx + ny * ly + nz * lz).max(0.0);

        // Halfway vector for specular
        let vx = 0.0;
        let vy = 0.0;
        let vz = 1.0;
        let hx = lx + vx;
        let hy = ly + vy;
        let hz = lz + vz;
        let h_len = (hx * hx + hy * hy + hz * hz).sqrt();
        let (hx, hy, hz) = (hx / h_len, hy / h_len, hz / h_len);
        let spec_val = (nx * hx + ny * hy + nz * hz).max(0.0).powf(gloss) * spec;

        let col = (diff + spec_val).clamp(0.0, 1.0);
        [col, col, col, 1.0]
    })])
}

fn cook_extend_shape(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let dist = context.f("distance");
    let smooth = context.f("smoothness").clamp(0.001, 1.0);
    let tile = context.tile;
    let dist_px = (dist * (w.min(h) as f32)).round() as i64;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let center = input.get_gray(x, y, tile);
        if dist_px == 0 {
            return center;
        }

        let rad = dist_px.abs();
        let mut best = center;
        for dy in -rad..=rad {
            for dx in -rad..=rad {
                if dx * dx + dy * dy <= rad * rad {
                    let v = input.get_gray(x + dx, y + dy, tile);
                    if dist_px > 0 {
                        best = best.max(v);
                    } else {
                        best = best.min(v);
                    }
                }
            }
        }

        let t = (best - center) * (1.0 - smooth);
        (center + t).clamp(0.0, 1.0)
    })])
}
