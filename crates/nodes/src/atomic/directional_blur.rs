use super::{ATOMIC, box_filter, direction, input_resize};
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Image;
use materialcraft_core::Value;

pub(super) fn definition() -> NodeDefinition {
    node("directional_blur", "Directional blur", ATOMIC)
        .desc("Blurs along one direction.")
        .atomic()
        .any_in("input", "Input")
        .float("intensity", "Intensity", 10.0, 0.0, 64.0)
        .angle("angle", "Angle", 0.0)
        .any_out()
        .cook(directional_blur)
}

fn directional_blur(context: &CookContext) -> CookResult {
    blur(context, context.f("angle"))
}

pub(super) fn blur(context: &CookContext, angle: f32) -> CookResult {
    let intensity = context.f("intensity");
    let (input, source_format) = input_resize::blur_input(context, intensity);
    const BASE_RESOLUTION: f32 = 256.0;
    const MAX_HALF_SAMPLES: i32 = 512;
    let stretched = context.w != context.h && context.vars.get("$pixelratio").and_then(Value::as_i32) == Some(0);
    let extent = |side: u32| (intensity.max(0.0) * side as f32 / BASE_RESOLUTION - 0.5).max(0.0);
    let [extent_x, extent_y] = if stretched { [context.w, context.h].map(extent) } else { [extent(context.w.min(context.h)); 2] };
    if extent_x == 0.0 && extent_y == 0.0 {
        return Ok(vec![input]);
    }

    let tile = context.tiling_axes();
    let radians = angle * std::f32::consts::TAU;
    if !radians.is_finite() {
        return Err("directional blur angle exceeds the finite radian range".into());
    }
    let sine = projected_sine(radians.abs()) * radians.signum();
    let cosine = projected_sine(radians.abs() + std::f32::consts::FRAC_PI_2);
    let quarters = angle * 4.0;
    let cardinal = (quarters.fract() == 0.0).then_some(quarters % 2.0 == 0.0);
    // On a rectangle one pixel extent can vanish before projection. Its surviving axis sets the tap layout.
    let axis = if extent_x == 0.0 {
        Some(false)
    } else if extent_y == 0.0 {
        Some(true)
    } else {
        cardinal
    };
    if let Some(horizontal) = axis {
        let projection = if cardinal == axis {
            // Rotated axial footprints project their reach one float below unity before normalization.
            if angle == 0.0 { 1.0 } else { f32::from_bits(0x3f7f_ffff) }
        } else if horizontal {
            cosine.abs()
        } else {
            sine.abs()
        };
        let reach = (if horizontal { extent_x } else { extent_y }) * projection;
        if reach == 0.0 {
            return Ok(vec![input]);
        }
        if reach > 2.0 {
            return Ok(vec![box_filter::axis(&input, reach, horizontal, tile[usize::from(!horizontal)], source_format, context.format)]);
        }
        let step = if !horizontal {
            [0.0, -1.0]
        } else if reach <= 1.0 {
            [-1.0, 0.0]
        } else {
            [1.0, 0.0]
        };
        return Ok(vec![box_filter::short_line(&input, reach, step, tile, source_format)]);
    }

    let (reach_x, reach_y) = (extent_x * cosine, extent_y * sine);
    let short_reach = reach_x.abs().max(reach_y.abs());
    let (sample_x, sample_y) = if stretched { (reach_x, reach_y) } else { (cosine, sine) };
    if short_reach <= 1.0 {
        // Normalize the offsets in texture coordinates so the longer side keeps its texel scale.
        let u = sample_x / context.w as f32;
        let v = sample_y / context.h as f32;
        // Tap order affects the fused sum. A rectangular image can change which UV axis dominates.
        let dominant = if u.abs() > v.abs() { u } else { v };
        if dominant == 0.0 {
            return Ok(vec![input]);
        }
        let span = -dominant;
        return Ok(vec![box_filter::short_line(&input, short_reach, [u / span, v / span], tile, source_format)]);
    }
    if tile == [true; 2] && context.w == input.width() && context.h == input.height() {
        // Compare the components before rounding the direction to the sample grid.
        let (line_y, line_x) = f64::from(radians).sin_cos();
        let horizontal = line_x.abs() * f64::from(extent_x) / f64::from(context.w) > line_y.abs() * f64::from(extent_y) / f64::from(context.h);
        // An authored eighth turn has an exact unit slope even after radian conversion.
        // On a rectangle the dominant UV direction can have the smaller texel
        // component. The window spans the projection along the chosen texel axis.
        // Each projected extent rounds before the divide that sets the shear slope.
        let (across, along) = if horizontal { (reach_y, reach_x) } else { (reach_x, reach_y) };
        let slope = if !stretched && (angle * 8.0).fract() == 0.0 { (line_y * line_x).signum() as f32 } else { across / along };
        if let Some(image) = box_filter::sheared(&input, along.abs(), horizontal, slope, source_format, context.format) {
            return Ok(vec![image]);
        }
    }
    let (dx, dy) = direction(angle);
    // Image-space rotation follows increasing rows.
    let (dx, dy) = if stretched { (extent_x * dx, -extent_y * dy) } else { (dx, -dy) };
    let major = dx.abs().max(dy.abs());
    let reach = if stretched { major } else { extent_x * major };
    let projected_radius = 0.5 + reach;
    let half_samples = (projected_radius + 0.5).floor().clamp(1.0, MAX_HALF_SAMPLES as f32) as i32;

    // Bound oblique sampling cost while preserving the extent of large kernels
    let step = ((projected_radius - 0.5) / half_samples as f32).max(1.0);
    let weight = step / (2.0 * projected_radius);
    let edge_weight = (0.5 - (half_samples as f32 - 0.5) * weight).max(0.0);
    let (inverse_width, inverse_height) = (1.0 / context.w as f32, 1.0 / context.h as f32);

    Ok(vec![Image::generate_uv(context.w, context.h, input.kind(), |u, v| {
        let mut pixel = [0.0; 4];
        for i in -half_samples..=half_samples {
            let sample_weight = if i.abs() == half_samples { edge_weight } else { weight };
            if sample_weight == 0.0 {
                continue;
            }

            let offset = i as f32 * step / major;
            let sample = input.sample_axes(u + dx * offset * inverse_width, v + dy * offset * inverse_height, tile);
            for channel in 0..4 {
                pixel[channel] += sample[channel] * sample_weight;
            }
        }
        pixel
    })])
}

/// The filter stores both direction components before deriving sample weights and coordinates.
fn projected_sine(radians: f32) -> f32 {
    use std::f64::consts::PI;

    const PI_HIGH: f32 = 3.140625;
    const PI_MIDDLE: f32 = (PI - PI_HIGH as f64) as f32;
    const PI_LOW: f32 = (PI - PI_HIGH as f64 - PI_MIDDLE as f64) as f32;

    // Large angles use a wide reduction before storing the direction component.
    if radians >= 10_000.0 {
        return f64::from(radians).sin() as f32;
    }
    let half_turns = (f64::from(radians) / PI).round_ties_even() as u32;
    // Each subtraction rounds separately. The last correction can distinguish angles
    // that would have the same single-precision remainder after one subtraction.
    let factor = -(half_turns as f32);
    let high = factor.mul_add(PI_HIGH, radians);
    let middle = factor.mul_add(PI_MIDDLE, high);
    let reduced = factor.mul_add(PI_LOW, middle);
    let x = if half_turns.is_multiple_of(2) { reduced } else { -reduced };
    let square = x * x;
    // Evaluate the odd sine series on the central half turn. Each product rounds before the next sum.
    let mut tail = -1.0 / 39_916_800.0;
    for coefficient in [1.0 / 362_880.0, -1.0 / 5_040.0, 1.0 / 120.0, -1.0 / 6.0] {
        tail = tail * square + coefficient;
    }
    x + (tail * square) * x
}
