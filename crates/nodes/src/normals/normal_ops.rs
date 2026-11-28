use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition_facing_normal() -> NodeDefinition {
    node("facing_normal", "Facing Normal", CATEGORY)
        .desc("Extracts the frontal alignment (dot product with view vector [0, 0, 1]) from a tangent normal map.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .float("falloff", "Falloff / Contrast", 1.0, 0.1, 10.0)
        .gray_out()
        .cook(cook_facing_normal)
}

pub(crate) fn definition_height_normal_blender() -> NodeDefinition {
    node("height_normal_blender", "Height Normal Blender", CATEGORY)
        .desc("Blends a base normal map with a new height detail layer with slope weighting.")
        .color_in("base_normal", "Base Normal")
        .primary_input("base_normal")
        .gray_in("detail_height", "Detail Height")
        .float("detail_intensity", "Detail Intensity", 1.0, 0.0, 10.0)
        .float("blend_weight", "Blend Weight", 0.5, 0.0, 1.0)
        .color_out()
        .cook(cook_height_normal_blender)
}

pub(crate) fn definition_height_to_normal_world_units() -> NodeDefinition {
    node("height_to_normal_world_units", "Height to Normal World Units", CATEGORY)
        .desc("Converts height map to normal map calibrated to physical world units (meters, centimeters).")
        .gray_in("height", "Height Map")
        .primary_input("height")
        .float("physical_size", "Tile Size (Meters)", 1.0, 0.001, 100.0)
        .float("physical_depth", "Relief Depth (Meters)", 0.1, 0.0001, 10.0)
        .boolean("directx", "DirectX Normals (-Y)", true)
        .color_out()
        .cook(cook_height_to_normal_world_units)
}

pub(crate) fn definition_normal_blend() -> NodeDefinition {
    node("normal_blend", "Normal Blend", CATEGORY)
        .desc("Lerps two normal maps with mask weighting and angle preservation.")
        .color_in("base", "Base Normal")
        .primary_input("base")
        .color_in("top", "Top Normal")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_normal_blend)
}

pub(crate) fn definition_normal_sobel() -> NodeDefinition {
    node("normal_sobel", "Normal Sobel", CATEGORY)
        .desc("High precision 3x3 Sobel kernel gradient operator for height-to-normal conversion.")
        .gray_in("height", "Height Map")
        .primary_input("height")
        .float("intensity", "Normal Intensity", 1.0, 0.0, 10.0)
        .color_out()
        .cook(cook_normal_sobel)
}

pub(crate) fn definition_normal_to_height_hq() -> NodeDefinition {
    node("normal_to_height_hq", "Normal To Height HQ", CATEGORY)
        .desc("Multi-scale Poisson height integration from normal derivatives with high-frequency detail retention.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .float("relief_depth", "Relief Depth", 1.0, 0.0, 5.0)
        .int("iterations", "Solver Iterations", 64, 8, 256)
        .gray_out()
        .cook(cook_normal_to_height_hq)
}

pub(crate) fn definition_normal_transform() -> NodeDefinition {
    node("normal_transform", "Normal Transform", CATEGORY)
        .desc("Applies 2D rotation or scale to a normal map while correctly re-orienting tangent vectors.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .angle("rotation", "Rotation Angle", 0.0)
        .float2("scale", "Scale", [1.0, 1.0], 0.1, 10.0)
        .color_out()
        .cook(cook_normal_transform)
}

pub(crate) fn definition_normal_vector_rotation() -> NodeDefinition {
    node("normal_vector_rotation", "Normal Vector Rotation", CATEGORY)
        .desc("Rotates the 3D vectors of a normal map around the Z axis.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .angle("angle", "Vector Rotation Angle", 0.0)
        .color_out()
        .cook(cook_normal_vector_rotation)
}

pub(crate) fn definition_bent_normal() -> NodeDefinition {
    node("bent_normal", "Bent Normal", CATEGORY)
        .desc("Generates bent normal maps pointing toward the mean unoccluded hemisphere direction.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .gray_in("height", "Height Map")
        .float("radius", "Sampling Radius", 0.2, 0.01, 1.0)
        .color_out()
        .cook(cook_bent_normal)
}

fn cook_facing_normal(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w, context.h);
    let falloff = context.f("falloff").clamp(0.1, 20.0);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = normal.get(x, y, tile);
        let nz = (n[2] * 2.0 - 1.0).clamp(0.0, 1.0);
        nz.powf(falloff)
    })])
}

fn cook_height_normal_blender(context: &CookContext<'_>) -> CookResult {
    let base = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let detail_h = context.any_or(1, Kind::Gray, [0.5; 4]);
    let (w, h) = (context.w, context.h);
    let intensity = context.f("detail_intensity").max(0.0);
    let weight = context.f("blend_weight").clamp(0.0, 1.0);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let b = base.get(x, y, tile);
        let bx = b[0] * 2.0 - 1.0;
        let by = b[1] * 2.0 - 1.0;
        let bz = (b[2] * 2.0 - 1.0).max(0.01);

        let r = detail_h.get_gray(x + 1, y, tile);
        let l = detail_h.get_gray(x - 1, y, tile);
        let d = detail_h.get_gray(x, y + 1, tile);
        let u = detail_h.get_gray(x, y - 1, tile);

        let dx = (r - l) * intensity * 0.5;
        let dy = (d - u) * intensity * 0.5;

        let rx = bx + dx * weight;
        let ry = by + dy * weight;
        let rz = bz;

        let len = (rx * rx + ry * ry + rz * rz).sqrt();
        let (nx, ny, nz) = if len > 1e-5 { (rx / len, ry / len, rz / len) } else { (0.0, 0.0, 1.0) };
        [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5, 1.0]
    })])
}

fn cook_height_to_normal_world_units(context: &CookContext<'_>) -> CookResult {
    let height = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let size_m = context.f("physical_size").clamp(0.001, 1000.0);
    let depth_m = context.f("physical_depth").clamp(0.0001, 100.0);
    let directx = context.b("directx");
    let tile = context.tile;

    let pixel_size_m = size_m / (w.max(1) as f32);
    let height_scale = depth_m / pixel_size_m;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let r = height.get_gray(x + 1, y, tile);
        let l = height.get_gray(x - 1, y, tile);
        let d = height.get_gray(x, y + 1, tile);
        let u = height.get_gray(x, y - 1, tile);

        let dx = (r - l) * height_scale * 0.5;
        let dy = (d - u) * height_scale * 0.5;
        let (nx, ny, nz) = (-dx, if directx { dy } else { -dy }, 1.0);
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len * 0.5 + 0.5, ny / len * 0.5 + 0.5, nz / len * 0.5 + 0.5, 1.0]
    })])
}

fn cook_normal_blend(context: &CookContext<'_>) -> CookResult {
    let base = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let top = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let mask = context.sized(2).map(|img| img.to_gray());
    let (w, h) = (context.w, context.h);
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let b = base.get(x, y, tile);
        let t = top.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |img| img.get_gray(x, y, tile)) * opacity;

        let bx = b[0] * 2.0 - 1.0;
        let by = b[1] * 2.0 - 1.0;
        let bz = b[2] * 2.0 - 1.0;

        let tx = t[0] * 2.0 - 1.0;
        let ty = t[1] * 2.0 - 1.0;
        let tz = t[2] * 2.0 - 1.0;

        let rx = bx + (tx - bx) * m;
        let ry = by + (ty - by) * m;
        let rz = bz + (tz - bz) * m;
        let len = (rx * rx + ry * ry + rz * rz).sqrt();
        let (nx, ny, nz) = if len > 1e-5 { (rx / len, ry / len, rz / len) } else { (0.0, 0.0, 1.0) };
        [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5, 1.0]
    })])
}

fn cook_normal_sobel(context: &CookContext<'_>) -> CookResult {
    let height = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let intensity = context.f("intensity").max(0.0);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let tl = height.get_gray(x - 1, y - 1, tile);
        let tc = height.get_gray(x, y - 1, tile);
        let tr = height.get_gray(x + 1, y - 1, tile);
        let ml = height.get_gray(x - 1, y, tile);
        let mr = height.get_gray(x + 1, y, tile);
        let bl = height.get_gray(x - 1, y + 1, tile);
        let bc = height.get_gray(x, y + 1, tile);
        let br = height.get_gray(x + 1, y + 1, tile);

        let dx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
        let dy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);

        let nx = -dx * intensity * 0.25;
        let ny = dy * intensity * 0.25;
        let nz = 1.0f32;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len * 0.5 + 0.5, ny / len * 0.5 + 0.5, nz / len * 0.5 + 0.5, 1.0]
    })])
}

fn cook_normal_to_height_hq(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w, context.h);
    let relief = context.f("relief_depth").max(0.0);
    let iterations = context.i("iterations").clamp(8, 256) as usize;
    let tile = context.tile;

    let total = (w * h) as usize;
    let mut divergence = vec![0.0f32; total];
    let mut height = vec![0.5f32; total];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let r = normal.get(xi + 1, yi, tile);
            let l = normal.get(xi - 1, yi, tile);
            let d = normal.get(xi, yi + 1, tile);
            let u = normal.get(xi, yi - 1, tile);

            let gx = (r[0] - l[0]) * relief;
            let gy = (d[1] - u[1]) * relief;
            divergence[(y * w + x) as usize] = (gx + gy) * 0.5;
        }
    }

    let mut next_h = height.clone();
    for _ in 0..iterations {
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                let r = height[(y * w + (x + 1) % w) as usize];
                let l = height[(y * w + (x + w - 1) % w) as usize];
                let d = height[(((y + 1) % h) * w + x) as usize];
                let u = height[(((y + h - 1) % h) * w + x) as usize];

                next_h[idx] = 0.25 * (r + l + d + u - divergence[idx]);
            }
        }
        std::mem::swap(&mut height, &mut next_h);
    }

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Gray, height, &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_normal_transform(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w, context.h);
    let rot = context.f("rotation") * std::f32::consts::PI * 2.0;
    let scale = context.v2("scale");
    let tile = context.tile;

    let cos_r = rot.cos();
    let sin_r = rot.sin();
    let sx = scale[0].max(0.01);
    let sy = scale[1].max(0.01);

    Ok(vec![Image::generate_uv(w, h, Kind::Color, |u, v| {
        let x = (u - 0.5) / sx;
        let y = (v - 0.5) / sy;
        let su = (cos_r * x - sin_r * y) + 0.5;
        let sv = (sin_r * x + cos_r * y) + 0.5;

        let n = normal.sample_axes(su, sv, [tile, tile]);
        let nx = n[0] * 2.0 - 1.0;
        let ny = n[1] * 2.0 - 1.0;
        let nz = n[2] * 2.0 - 1.0;

        // Rotate the normal vector by the same rotation angle
        let r_nx = cos_r * nx - sin_r * ny;
        let r_ny = sin_r * nx + cos_r * ny;

        [r_nx * 0.5 + 0.5, r_ny * 0.5 + 0.5, nz * 0.5 + 0.5, n[3]]
    })])
}

fn cook_normal_vector_rotation(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w, context.h);
    let rot = context.f("angle") * std::f32::consts::PI * 2.0;
    let tile = context.tile;

    let cos_r = rot.cos();
    let sin_r = rot.sin();

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = normal.get(x, y, tile);
        let nx = n[0] * 2.0 - 1.0;
        let ny = n[1] * 2.0 - 1.0;
        let nz = n[2] * 2.0 - 1.0;

        let r_nx = cos_r * nx - sin_r * ny;
        let r_ny = sin_r * nx + cos_r * ny;

        [r_nx * 0.5 + 0.5, r_ny * 0.5 + 0.5, nz * 0.5 + 0.5, n[3]]
    })])
}

fn cook_bent_normal(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let height = context.any_or(1, Kind::Gray, [0.5; 4]);
    let (w, h) = (context.w, context.h);
    let radius = context.f("radius").clamp(0.01, 1.0);
    let tile = context.tile;
    let rad_px = (radius * (w.min(h) as f32) * 0.5).max(1.0);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = normal.get(x, y, tile);
        let bx = n[0] * 2.0 - 1.0;
        let by = n[1] * 2.0 - 1.0;
        let bz = (n[2] * 2.0 - 1.0).max(0.01);
        let mut bx_off = 0.0f32;
        let mut by_off = 0.0f32;
        let h_c = height.get_gray(x, y, tile);

        // Sample 8 directional rays around hemisphere to find unoccluded mean direction
        for i in 0..8 {
            let phi = i as f32 * std::f32::consts::PI * 0.25;
            let sx = x + (phi.cos() * rad_px).round() as i64;
            let sy = y + (phi.sin() * rad_px).round() as i64;
            let h_s = height.get_gray(sx, sy, tile);
            let dh = h_s - h_c;
            if dh > 0.0 {
                // Occlusion pushes bent normal away from occluder
                bx_off -= phi.cos() * dh * 0.5;
                by_off -= phi.sin() * dh * 0.5;
            }
        }

        let bx = bx + bx_off;
        let by = by + by_off;

        let len = (bx * bx + by * by + bz * bz).sqrt();
        let (nx, ny, nz) = if len > 1e-5 { (bx / len, by / len, bz / len) } else { (0.0, 0.0, 1.0) };
        [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5, 1.0]
    })])
}
