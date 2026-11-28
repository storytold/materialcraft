use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn gradient_axial_reflected_definition() -> NodeDefinition {
    node("gradient_axial_reflected", "Gradient Axial Reflected", CATEGORY)
        .desc("Symmetric reflected axial gradient mirrored outward from a central axis.")
        .angle("angle", "Angle", 0.0)
        .float("tiling", "Tiling", 1.0, 0.1, 32.0)
        .gray_out()
        .cook(cook_gradient_axial_reflected)
}

pub(crate) fn gradient_radial_definition() -> NodeDefinition {
    node("gradient_radial", "Gradient Radial", CATEGORY)
        .desc("Conical 360-degree radial sweep gradient centered around a pivot point.")
        .angle("angle", "Angle Offset", 0.0)
        .boolean("clockwise", "Clockwise", true)
        .gray_out()
        .cook(cook_gradient_radial)
}

pub(crate) fn starburst_definition() -> NodeDefinition {
    node("starburst", "Starburst", CATEGORY)
        .desc("Radiating tapered ray burst generator with beam count, width, and falloff.")
        .int("rays", "Rays Count", 12, 3, 64)
        .float("inner_radius", "Inner Radius", 0.1, 0.0, 0.8)
        .float("outer_radius", "Outer Radius", 0.8, 0.1, 1.0)
        .float("beam_width", "Beam Width", 0.5, 0.05, 1.0)
        .angle("angle", "Rotation", 0.0)
        .gray_out()
        .cook(cook_starburst)
}

pub(crate) fn height_extrude_definition() -> NodeDefinition {
    node("height_extrude", "Height Extrude", CATEGORY)
        .desc("Extrudes and expands input shape boundaries proportional to a height ramp.")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("distance", "Extrude Distance", 0.1, 0.0, 1.0)
        .angle("direction", "Direction", 0.0)
        .gray_out()
        .cook(cook_height_extrude)
}

pub(crate) fn shape_extrude_definition() -> NodeDefinition {
    node("shape_extrude", "Shape Extrude", CATEGORY)
        .desc("Extrudes 2D shapes into 3D isometric/directional depth bevels.")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("depth", "Depth", 0.2, 0.0, 1.0)
        .angle("angle", "Light/Extrude Angle", 0.25)
        .gray_out()
        .cook(cook_shape_extrude)
}

pub(crate) fn shape_mapper_definition() -> NodeDefinition {
    node("shape_mapper", "Shape Mapper", CATEGORY)
        .desc("Transforms input pattern coordinates along polar, spiral, or custom path mappings.")
        .color_in("input", "Pattern")
        .primary_input("input")
        .choice("mapping", "Mapping Mode", &["Polar (Circle)", "Spiral", "Kaleidoscope"], 0)
        .int("repeats", "Repeats", 4, 1, 32)
        .color_out()
        .cook(cook_shape_mapper)
}

fn cook_gradient_axial_reflected(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let angle = context.f("angle") * PI;
    let tiling = context.f("tiling").clamp(0.1, 64.0);

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;
        let dist = (x * cos_a - y * sin_a).abs() * 2.0 * tiling;
        let val = (1.0 - dist).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_gradient_radial(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let angle_off = context.f("angle") * PI;
    let clockwise = context.b("clockwise");

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = -(v - 0.5);
        let mut theta = y.atan2(x) - angle_off;
        if !clockwise {
            theta = -theta;
        }
        let t = (theta / (2.0 * PI)).rem_euclid(1.0);
        [t, 0.0, 0.0, 1.0]
    })])
}

fn cook_starburst(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let rays = context.i("rays").clamp(3, 128) as f32;
    let r_in = context.f("inner_radius").clamp(0.0, 1.0) * 0.5;
    let r_out = context.f("outer_radius").clamp(0.01, 1.0) * 0.5;
    let width = context.f("beam_width").clamp(0.01, 1.0);
    let rot = context.f("angle") * PI;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;
        let dist = (x * x + y * y).sqrt();

        if dist < r_in || dist > r_out {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            let theta = y.atan2(x) - rot;
            let ray_t = (theta * (rays / (2.0 * PI))).rem_euclid(1.0);
            let ray_d = (ray_t - 0.5).abs() * 2.0;

            let beam = if ray_d <= width { (1.0 - ray_d / width).powf(1.5) } else { 0.0 };
            let radial_falloff = 1.0 - ((dist - r_in) / (r_out - r_in).max(1e-4));
            let val = (beam * radial_falloff).clamp(0.0, 1.0);
            [val, 0.0, 0.0, 1.0]
        }
    })])
}

fn cook_height_extrude(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let dist = context.f("distance").clamp(0.0, 1.0);
    let angle = context.f("direction") * PI;
    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    let dx = angle.cos() * dist * 0.1;
    let dy = angle.sin() * dist * 0.1;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut max_val = 0.0f32;
        for step in 0..8 {
            let t = step as f32 / 7.0;
            let s = input.sample_axes(u - dx * t, v - dy * t, tiling)[0];
            max_val = max_val.max(s * (1.0 - t * 0.3));
        }
        [max_val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_shape_extrude(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let depth = context.f("depth").clamp(0.0, 1.0);
    let angle = context.f("angle") * PI;
    let (w, h) = (context.w, context.h);
    let tiling = context.tiling_axes();

    let dx = angle.cos() * depth * 0.05;
    let dy = angle.sin() * depth * 0.05;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let top = input.sample_axes(u, v, tiling)[0];
        let mut shadow = 0.0f32;
        for step in 1..=6 {
            let t = step as f32 / 6.0;
            let s = input.sample_axes(u - dx * t, v - dy * t, tiling)[0];
            shadow = shadow.max(s * (1.0 - t * 0.5));
        }
        let val = top.max(shadow * 0.6);
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_shape_mapper(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [1.0; 4]);
    let mode = context.i("mapping");
    let repeats = context.i("repeats").clamp(1, 32) as f32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let x = u - 0.5;
        let y = -(v - 0.5);

        let r = (x * x + y * y).sqrt() * 2.0;
        let theta = y.atan2(x);

        let (map_u, map_v) = match mode {
            // Spiral
            1 => {
                let su = (theta / (2.0 * PI)).rem_euclid(1.0);
                let sv = (r + su * 0.5).rem_euclid(1.0);
                (su * repeats, sv)
            }
            // Kaleidoscope
            2 => {
                let sector = (2.0 * PI) / repeats;
                let folded = (theta.rem_euclid(sector) - sector * 0.5).abs();
                let su = (folded / (sector * 0.5)).clamp(0.0, 1.0);
                (su, r.min(1.0))
            }
            // Polar (Circle)
            _ => {
                let su = ((theta / (2.0 * PI)) * repeats).rem_euclid(1.0);
                (su, r.min(1.0))
            }
        };

        input.sample_axes(map_u.rem_euclid(1.0), map_v.clamp(0.0, 1.0), [true, false])
    })])
}
