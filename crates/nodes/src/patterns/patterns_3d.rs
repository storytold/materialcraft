use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn gradient_3d_linear_definition() -> NodeDefinition {
    node("gradient_3d_linear", "3D Linear Gradient", CATEGORY)
        .desc("Slices a 3D volumetric linear ramp gradient at a customizable depth plane.")
        .float("scale", "Scale", 1.0, 0.1, 10.0)
        .float("z_depth", "Z Depth", 0.0, -2.0, 2.0)
        .angle("angle_x", "Pitch Angle", 0.0)
        .angle("angle_y", "Yaw Angle", 0.0)
        .gray_out()
        .cook(cook_gradient_3d_linear)
}

pub(crate) fn volume_mask_3d_definition() -> NodeDefinition {
    node("volume_mask_3d", "3D Volume Mask", CATEGORY)
        .desc("Slices a 3D sphere or box volume mask at a customizable Z depth plane.")
        .choice("shape", "Shape", &["Sphere", "Box", "Cylinder"], 0)
        .float("radius", "Radius / Size", 0.4, 0.05, 1.0)
        .float("z_slice", "Z Slice", 0.0, -1.0, 1.0)
        .float("feather", "Feathering", 0.05, 0.0, 0.5)
        .gray_out()
        .cook(cook_volume_mask_3d)
}

pub(crate) fn cube_3d_definition() -> NodeDefinition {
    node("cube_3d", "Cube 3D", CATEGORY)
        .desc("Renders an orthographic 3D cube with customizable rotation angles and diffuse shading.")
        .float("size", "Size", 0.5, 0.1, 1.0)
        .angle("pitch", "Pitch", 0.15)
        .angle("yaw", "Yaw", 0.25)
        .angle("roll", "Roll", 0.0)
        .color_out()
        .cook(cook_cube_3d)
}

pub(crate) fn cube_3d_gbuffers_definition() -> NodeDefinition {
    node("cube_3d_gbuffers", "Cube 3D GBuffers", CATEGORY)
        .desc("Outputs world-space surface normals and depth G-buffers for a rotating 3D cube.")
        .float("size", "Size", 0.5, 0.1, 1.0)
        .angle("pitch", "Pitch", 0.15)
        .angle("yaw", "Yaw", 0.25)
        .angle("roll", "Roll", 0.0)
        .named_color_out("normals", "World Normals")
        .named_gray_out("depth", "Depth")
        .cook(cook_cube_3d_gbuffers)
}

pub(crate) fn panorama_shape_definition() -> NodeDefinition {
    node("panorama_shape", "Panorama Shape", CATEGORY)
        .desc("Maps spherical coordinates (equirectangular latitude/longitude) to planar panorama view.")
        .float("radius", "Radius", 0.3, 0.01, 1.0)
        .float("latitude", "Center Latitude", 0.0, -1.0, 1.0)
        .float("longitude", "Center Longitude", 0.0, -1.0, 1.0)
        .gray_out()
        .cook(cook_panorama_shape)
}

fn cook_gradient_3d_linear(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(0.01, 20.0);
    let z_depth = context.f("z_depth");
    let ax = context.f("angle_x") * PI;
    let ay = context.f("angle_y") * PI;

    let nx = ax.cos() * ay.cos();
    let ny = ax.sin();
    let nz = ax.cos() * ay.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = (u - 0.5) * scale;
        let y = (v - 0.5) * scale;
        let z = z_depth * scale;
        let proj = x * nx + y * ny + z * nz + 0.5;
        [proj.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_volume_mask_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let shape = context.i("shape");
    let r = context.f("radius").clamp(0.01, 2.0);
    let z = context.f("z_slice");
    let feather = context.f("feather").clamp(0.001, 1.0);

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;
        let dist = match shape {
            // Box
            1 => x.abs().max(y.abs()).max(z.abs()),
            // Cylinder
            2 => (x * x + y * y).sqrt().max(z.abs()),
            // Sphere (default)
            _ => (x * x + y * y + z * z).sqrt(),
        };

        let val = if dist <= r {
            1.0
        } else if dist <= r + feather {
            1.0 - (dist - r) / feather
        } else {
            0.0
        };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn intersect_cube(ro: [f32; 3], rd: [f32; 3], half_size: f32) -> Option<(f32, [f32; 3])> {
    let mut t_min = -f32::INFINITY;
    let mut t_max = f32::INFINITY;
    let mut normal = [0.0f32; 3];

    for i in 0..3 {
        let inv_d = 1.0 / rd[i];
        let mut t0 = (-half_size - ro[i]) * inv_d;
        let mut t1 = (half_size - ro[i]) * inv_d;
        let mut n0 = [0.0f32; 3];
        let mut n1 = [0.0f32; 3];
        n0[i] = -1.0;
        n1[i] = 1.0;

        if inv_d < 0.0 {
            std::mem::swap(&mut t0, &mut t1);
            std::mem::swap(&mut n0, &mut n1);
        }

        if t0 > t_min {
            t_min = t0;
            normal = n0;
        }
        t_max = t_max.min(t1);

        if t_max < t_min {
            return None;
        }
    }

    if t_min > 0.0 { Some((t_min, normal)) } else { None }
}

fn rotate_ray(ro: [f32; 3], rd: [f32; 3], pitch: f32, yaw: f32, roll: f32) -> ([f32; 3], [f32; 3]) {
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let (cr, sr) = (roll.cos(), roll.sin());

    let rot = |v: [f32; 3]| -> [f32; 3] {
        // Yaw
        let x1 = v[0] * cy - v[2] * sy;
        let z1 = v[0] * sy + v[2] * cy;
        // Pitch
        let y2 = v[1] * cp - z1 * sp;
        let z2 = v[1] * sp + z1 * cp;
        // Roll
        let x3 = x1 * cr - y2 * sr;
        let y3 = x1 * sr + y2 * cr;
        [x3, y3, z2]
    };

    (rot(ro), rot(rd))
}

fn cook_cube_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let size = context.f("size").clamp(0.05, 1.0) * 0.5;
    let pitch = context.f("pitch") * PI;
    let yaw = context.f("yaw") * PI;
    let roll = context.f("roll") * PI;

    let light = [0.577, 0.577, 0.577];

    Ok(vec![Image::generate_uv(w, h, Kind::Color, |u, v| {
        let x = u - 0.5;
        let y = -(v - 0.5);
        let ro = [x, y, -2.0];
        let rd = [0.0, 0.0, 1.0];

        let (ro_rot, rd_rot) = rotate_ray(ro, rd, pitch, yaw, roll);
        if let Some((_, norm)) = intersect_cube(ro_rot, rd_rot, size) {
            let dot = (norm[0] * light[0] + norm[1] * light[1] + norm[2] * light[2]).max(0.0);
            let shade = (dot * 0.7 + 0.3).clamp(0.0, 1.0);
            [shade, shade, shade, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    })])
}

fn cook_cube_3d_gbuffers(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let size = context.f("size").clamp(0.05, 1.0) * 0.5;
    let pitch = context.f("pitch") * PI;
    let yaw = context.f("yaw") * PI;
    let roll = context.f("roll") * PI;

    let mut normals = vec![[0.0f32; 4]; (w * h) as usize];
    let mut depth = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let py = -(v - 0.5);
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let px = u - 0.5;
            let ro = [px, py, -2.0];
            let rd = [0.0, 0.0, 1.0];

            let (ro_rot, rd_rot) = rotate_ray(ro, rd, pitch, yaw, roll);
            let idx = (y * w + x) as usize;
            if let Some((t, norm)) = intersect_cube(ro_rot, rd_rot, size) {
                normals[idx] = [norm[0] * 0.5 + 0.5, norm[1] * 0.5 + 0.5, norm[2] * 0.5 + 0.5, 1.0];
                depth[idx] = (t / 4.0).clamp(0.0, 1.0);
            }
        }
    }

    let limits = craft_texture::Limits::default();
    let norm_img = Image::from_data(w, h, Kind::Color, normals.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?;
    let depth_img = Image::from_data(w, h, Kind::Gray, depth, &limits).map_err(|e| e.to_string())?;

    Ok(vec![norm_img, depth_img])
}

fn cook_panorama_shape(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let r = context.f("radius").clamp(0.01, 1.0);
    let lat_center = context.f("latitude") * PI * 0.5;
    let lon_center = context.f("longitude") * PI;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let lon = (u - 0.5) * 2.0 * PI;
        let lat = (0.5 - v) * PI;

        // Spherical angular distance (Great-circle distance)
        let d_lon = (lon - lon_center).abs();
        let cos_dist = lat.sin() * lat_center.sin() + lat.cos() * lat_center.cos() * d_lon.cos();
        let angle_dist = cos_dist.clamp(-1.0, 1.0).acos();

        let val = if angle_dist <= r { (1.0 - angle_dist / r).clamp(0.0, 1.0) } else { 0.0 };
        [val, 0.0, 0.0, 1.0]
    })])
}
