use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const CATEGORY: &str = "Filters/Transforms";

pub(crate) fn definition_cartesian_to_polar() -> NodeDefinition {
    node("cartesian_to_polar", "Cartesian To Polar", CATEGORY)
        .desc("Maps rectangular Cartesian coordinates (X, Y) to circular polar coordinates (Angle, Radius).")
        .any_in("input", "Input")
        .primary_input("input")
        .angle("angle_offset", "Angle Offset", 0.0)
        .float("radius_scale", "Radius Scale", 1.0, 0.1, 10.0)
        .any_out()
        .cook(cook_cartesian_to_polar)
}

pub(crate) fn definition_polar_to_cartesian() -> NodeDefinition {
    node("polar_to_cartesian", "Polar to Cartesian", CATEGORY)
        .desc("Converts polar concentric coordinates (Radius, Angle) into rectangular Cartesian layout.")
        .any_in("input", "Input")
        .primary_input("input")
        .angle("angle_offset", "Angle Offset", 0.0)
        .any_out()
        .cook(cook_polar_to_cartesian)
}

pub(crate) fn definition_clone() -> NodeDefinition {
    node("clone", "Clone", CATEGORY)
        .desc("Clones and stamps a pattern patch from source coordinates onto target coordinates with soft blending.")
        .any_in("input", "Source Pattern")
        .primary_input("input")
        .float2("source_pos", "Source Center", [0.5, 0.5], 0.0, 1.0)
        .float2("target_pos", "Target Center", [0.5, 0.5], 0.0, 1.0)
        .float("radius", "Stamp Radius", 0.2, 0.01, 1.0)
        .float("feather", "Feather Softness", 0.1, 0.0, 0.5)
        .any_out()
        .cook(cook_clone)
}

pub(crate) fn definition_mirror() -> NodeDefinition {
    node("mirror", "Mirror", CATEGORY)
        .desc("Mirrors input image across horizontal, vertical or arbitrary angle axes.")
        .any_in("input", "Input")
        .primary_input("input")
        .choice("axis", "Axis", &["Horizontal", "Vertical", "Both"], 0)
        .float("center", "Center Offset", 0.5, 0.0, 1.0)
        .any_out()
        .cook(cook_mirror)
}

pub(crate) fn definition_safe_transform() -> NodeDefinition {
    node("safe_transform", "Safe Transform", CATEGORY)
        .desc("Applies 2D rotation, scale and translation while enforcing seamless image bounds and integer mip alignments.")
        .any_in("input", "Input")
        .primary_input("input")
        .float2("offset", "Offset", [0.0, 0.0], -1.0, 1.0)
        .float2("scale", "Scale", [1.0, 1.0], 0.1, 10.0)
        .angle("angle", "Angle", 0.0)
        .any_out()
        .cook(cook_safe_transform)
}

pub(crate) fn definition_non_square_transform() -> NodeDefinition {
    node("non_square_transform", "Non-Square Transform", CATEGORY)
        .desc("Adapts non-square aspect ratio textures (1:2, 2:1) into square formats with aspect compensation.")
        .any_in("input", "Input")
        .primary_input("input")
        .choice("ratio", "Aspect Ratio", &["2:1", "1:2", "4:1", "1:4"], 0)
        .any_out()
        .cook(cook_non_square_transform)
}

pub(crate) fn definition_non_uniform_rotation() -> NodeDefinition {
    node("non_uniform_rotation", "Non-Uniform Rotation", CATEGORY)
        .desc("Rotates an image with distinct independent stretching scales along the X and Y axes.")
        .any_in("input", "Input")
        .primary_input("input")
        .angle("angle", "Rotation Angle", 0.0)
        .float("scale_x", "Scale X", 1.0, 0.1, 10.0)
        .float("scale_y", "Scale Y", 1.0, 0.1, 10.0)
        .any_out()
        .cook(cook_non_uniform_rotation)
}

pub(crate) fn definition_skew() -> NodeDefinition {
    node("skew", "Skew", CATEGORY)
        .desc("Shears texture coordinates horizontally or vertically by an angular offset.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("skew_x", "Skew X", 0.0, -2.0, 2.0)
        .float("skew_y", "Skew Y", 0.0, -2.0, 2.0)
        .any_out()
        .cook(cook_skew)
}

pub(crate) fn definition_symmetry() -> NodeDefinition {
    node("symmetry", "Symmetry", CATEGORY)
        .desc("Creates 2, 4, or 8-fold radial or axial kaleidoscopic symmetry reflections.")
        .any_in("input", "Input")
        .primary_input("input")
        .int("sectors", "Symmetry Sectors", 4, 2, 16)
        .angle("angle", "Angle Offset", 0.0)
        .any_out()
        .cook(cook_symmetry)
}

pub(crate) fn definition_symmetry_slice() -> NodeDefinition {
    node("symmetry_slice", "Symmetry Slice", CATEGORY)
        .desc("Slices half of an input along an angled knife plane and mirrors it onto the other half.")
        .any_in("input", "Input")
        .primary_input("input")
        .angle("angle", "Slice Angle", 0.0)
        .float("pivot", "Pivot Offset", 0.5, 0.0, 1.0)
        .any_out()
        .cook(cook_symmetry_slice)
}

pub(crate) fn definition_trapezoid_transform() -> NodeDefinition {
    node("trapezoid_transform", "Trapezoid Transform", CATEGORY)
        .desc("Pinches or expands opposite texture edges to simulate trapezoidal perspective distortion.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("top_scale", "Top Edge Scale", 1.0, 0.01, 4.0)
        .float("bottom_scale", "Bottom Edge Scale", 1.0, 0.01, 4.0)
        .any_out()
        .cook(cook_trapezoid_transform)
}

pub(crate) fn definition_quad_transform() -> NodeDefinition {
    node("quad_transform", "Quad Transform", CATEGORY)
        .desc("Arbitrary 4-point projective homography mapping each corner of a texture.")
        .any_in("input", "Input")
        .primary_input("input")
        .float2("corner_tl", "Top Left Corner", [0.0, 0.0], -0.5, 1.5)
        .float2("corner_tr", "Top Right Corner", [1.0, 0.0], -0.5, 1.5)
        .float2("corner_br", "Bottom Right Corner", [1.0, 1.0], -0.5, 1.5)
        .float2("corner_bl", "Bottom Left Corner", [0.0, 1.0], -0.5, 1.5)
        .any_out()
        .cook(cook_quad_transform)
}

fn cook_cartesian_to_polar(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let angle_off = context.f("angle_offset") * PI * 2.0;
    let rad_scale = context.f("radius_scale").max(0.01);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let dx = (u - 0.5) * 2.0;
        let dy = (v - 0.5) * 2.0;
        let r = (dx * dx + dy * dy).sqrt() * rad_scale;
        let theta = (dy.atan2(dx) + angle_off).rem_euclid(2.0 * PI) / (2.0 * PI);
        input.sample_axes(theta, r, tiling)
    })])
}

fn cook_polar_to_cartesian(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let angle_off = context.f("angle_offset") * PI * 2.0;
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let theta = u * 2.0 * PI + angle_off;
        let r = v * 0.5;
        let su = 0.5 + r * theta.cos();
        let sv = 0.5 + r * theta.sin();
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_clone(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let src = context.v2("source_pos");
    let dst = context.v2("target_pos");
    let rad = context.f("radius").clamp(0.001, 1.0);
    let feather = context.f("feather").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let off_x = dst[0] - src[0];
    let off_y = dst[1] - src[1];

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let dist = ((u - dst[0]).powi(2) + (v - dst[1]).powi(2)).sqrt();
        let base_val = input.sample_axes(u, v, tiling);
        if dist < rad + feather {
            let su = u - off_x;
            let sv = v - off_y;
            let clone_val = input.sample_axes(su, sv, tiling);
            let w = if dist <= rad {
                1.0
            } else if feather > 1e-4 {
                1.0 - (dist - rad) / feather
            } else {
                0.0
            };
            [
                base_val[0] * (1.0 - w) + clone_val[0] * w,
                base_val[1] * (1.0 - w) + clone_val[1] * w,
                base_val[2] * (1.0 - w) + clone_val[2] * w,
                base_val[3] * (1.0 - w) + clone_val[3] * w,
            ]
        } else {
            base_val
        }
    })])
}

fn cook_mirror(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let axis = context.i("axis");
    let center = context.f("center").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let su = match axis {
            0 | 2 => center - (u - center).abs(),
            _ => u,
        };
        let sv = match axis {
            1 | 2 => center - (v - center).abs(),
            _ => v,
        };
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_safe_transform(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let offset = context.v2("offset");
    let scale = context.v2("scale");
    let angle = context.f("angle") * PI * 2.0;
    let tiling = context.tiling_axes();

    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let sx = scale[0].max(0.001);
    let sy = scale[1].max(0.001);

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let x = (u - 0.5) / sx;
        let y = (v - 0.5) / sy;
        let su = cos_a * x - sin_a * y + 0.5 - offset[0];
        let sv = sin_a * x + cos_a * y + 0.5 - offset[1];
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_non_square_transform(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let ratio = context.i("ratio");
    let tiling = context.tiling_axes();

    let (sx, sy) = match ratio {
        0 => (2.0f32, 1.0f32), // 2:1
        1 => (1.0f32, 2.0f32), // 1:2
        2 => (4.0f32, 1.0f32), // 4:1
        _ => (1.0f32, 4.0f32), // 1:4
    };

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| input.sample_axes(u * sx, v * sy, tiling))])
}

fn cook_non_uniform_rotation(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let angle = context.f("angle") * PI * 2.0;
    let sx = context.f("scale_x").max(0.01);
    let sy = context.f("scale_y").max(0.01);
    let tiling = context.tiling_axes();

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let x = (u - 0.5) / sx;
        let y = (v - 0.5) / sy;
        let su = cos_a * x - sin_a * y + 0.5;
        let sv = sin_a * x + cos_a * y + 0.5;
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_skew(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let skew_x = context.f("skew_x");
    let skew_y = context.f("skew_y");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let su = u - skew_x * (v - 0.5);
        let sv = v - skew_y * (u - 0.5);
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_symmetry(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let sectors = context.i("sectors").clamp(2, 32) as f32;
    let angle_off = context.f("angle") * PI * 2.0;
    let tiling = context.tiling_axes();

    let sector_angle = 2.0 * PI / sectors;

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        let theta = (dy.atan2(dx) + angle_off).rem_euclid(2.0 * PI);
        let sector_idx = (theta / sector_angle).floor();
        let mut local_theta = theta - sector_idx * sector_angle;
        if (sector_idx as i32) % 2 == 1 {
            local_theta = sector_angle - local_theta;
        }

        let su = 0.5 + r * local_theta.cos();
        let sv = 0.5 + r * local_theta.sin();
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_symmetry_slice(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let angle = context.f("angle") * PI * 2.0;
    let pivot = context.f("pivot").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let nx = angle.cos();
    let ny = angle.sin();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let d = (u - pivot) * nx + (v - pivot) * ny;
        let (su, sv) = if d < 0.0 { (u - 2.0 * d * nx, v - 2.0 * d * ny) } else { (u, v) };
        input.sample_axes(su, sv, tiling)
    })])
}

fn cook_trapezoid_transform(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let top = context.f("top_scale").clamp(0.01, 10.0);
    let bot = context.f("bottom_scale").clamp(0.01, 10.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        let width_at_v = top * (1.0 - v) + bot * v;
        let su = (u - 0.5) / width_at_v + 0.5;
        input.sample_axes(su, v, tiling)
    })])
}

fn cook_quad_transform(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let tl = context.v2("corner_tl");
    let tr = context.v2("corner_tr");
    let br = context.v2("corner_br");
    let bl = context.v2("corner_bl");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(w, h, input.kind(), |u, v| {
        // Bilinear interpolation of destination quad corners
        let top_x = tl[0] * (1.0 - u) + tr[0] * u;
        let top_y = tl[1] * (1.0 - u) + tr[1] * u;
        let bot_x = bl[0] * (1.0 - u) + br[0] * u;
        let bot_y = bl[1] * (1.0 - u) + br[1] * u;

        let su = top_x * (1.0 - v) + bot_x * v;
        let sv = top_y * (1.0 - v) + bot_y * v;
        input.sample_axes(su, sv, tiling)
    })])
}

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        definition_cartesian_to_polar(),
        definition_polar_to_cartesian(),
        definition_clone(),
        definition_mirror(),
        definition_safe_transform(),
        definition_non_square_transform(),
        definition_non_uniform_rotation(),
        definition_skew(),
        definition_symmetry(),
        definition_symmetry_slice(),
        definition_trapezoid_transform(),
        definition_quad_transform(),
    ]
}
