pub(super) fn circular(pattern: i32) -> bool {
    matches!(pattern, 3..=7 | 11..=14 | 16 | 18)
}

pub(super) fn radius_squared([x, y]: [f32; 2]) -> f32 {
    y.mul_add(y, x * x)
}

/// Prepare the per-stamp constants once, then sample coverage and the signed channel offset.
pub(super) fn prepare(pattern: i32, variation: f32) -> impl Fn(f32, f32) -> Result<Option<(f32, f32)>, String> {
    let width = match pattern {
        9 if variation > 0.0 => variation.max(f32::MIN_POSITIVE),
        12 | 14 => 1.0 - variation.max(0.0),
        _ => 0.0,
    };
    let inverse_width = if width == 0.0 { 0.0 } else { crate::function::pixel_divide(1.0, width) };
    let circular = circular(pattern);
    move |x: f32, y: f32| {
        let radius_squared = radius_squared([x, y]);
        // An uncovered pixel preserves the background even when the blend selects the maximum.
        if circular && radius_squared > 1.0 {
            return Ok(None);
        }
        profile(pattern, x, y, radius_squared, variation, inverse_width).map(Some)
    }
}

fn profile(pattern: i32, x: f32, y: f32, radius_squared: f32, variation: f32, inverse_width: f32) -> Result<(f32, f32), String> {
    let radial = (1.0 - radius_squared).max(0.0);
    let value = match pattern {
        2 | 3 => 1.0,
        4 => radial,
        5 => quartic_bell(radius_squared),
        6 => {
            const FALLOFF: f32 = 11.1 * std::f32::consts::LOG2_E;
            (crate::function::pixel_exp2(-FALLOFF * radius_squared) - 0.000_015_1).max(0.0)
        }
        7 => (1.0 - crate::function::pixel_sqrt(radius_squared)).max(0.0).powi(4),
        8 => (1.0 - x.abs().max(y.abs())).max(0.0),
        9 if variation <= 0.0 => 1.0,
        9 => {
            // Each edge fades independently, including at a corner.
            let fade = |coordinate: f32| {
                let t = ((1.0 - coordinate.abs()) * inverse_width).clamp(0.0, 1.0);
                let square = t * t;
                (-2.0_f32).mul_add(square * t, 3.0 * square)
            };
            fade(x) * fade(y)
        }
        10 => (x + 1.0) * 0.5 * (1.0 - variation.max(0.0)),
        11 => {
            // A finite frequency keeps the phase at x = 0 defined even for very large variations.
            let cycles = 6.0_f32.mul_add(variation.max(0.0), 1.5).min(f32::MAX);
            let phase = f64::from(x) * f64::from(cycles);
            let fraction = (phase - phase.round()).abs() as f32;
            let triangle = 1.0 - 4.0 * fraction;
            let parabola = (-triangle.abs()).mul_add(triangle, 2.0 * triangle);
            // The measured wave uses a quadratic correction before the radial fade.
            const CORRECTION: f32 = 0.223_936_68;
            let wave = parabola * CORRECTION.mul_add(parabola.abs(), 1.0 - CORRECTION);
            radial * (0.5 * wave + 0.5)
        }
        12 => quartic_bell(radius_squared) * descending_fade(x, inverse_width),
        13 => quartic_bell(radius_squared) * (1.0 - x.abs()).powi(2),
        14 => {
            let t = radius_squared.min(1.0);
            // Normalize t(1-t)² to one at t=1/3.
            let ring = 6.75_f32.mul_add(t, -13.5).mul_add(t, 6.75) * t;
            ring * descending_fade((x + 1.0) * 0.5, inverse_width)
        }
        15 => {
            let tip = (x.abs() - 0.77 * variation.max(0.0)).max(0.0);
            let distance_squared = y.mul_add(y, tip * tip);
            (-20.0_f32).mul_add(distance_squared, 1.0).max(0.0)
        }
        16 => (1.0 - crate::function::pixel_sqrt(radius_squared)).max(0.0),
        17 => return Ok(((x + 1.0) * 0.5, variation)),
        18 => return Ok((crate::function::pixel_sqrt(radial), variation)),
        _ => return Err(format!("unknown built-in pattern {pattern}")),
    };

    Ok((value, 0.0))
}

fn quartic_bell(radius_squared: f32) -> f32 {
    // The expanded quartic retains the signed rounding of the low-coverage skirt.
    let t = radius_squared.min(1.0);
    (t - 4.0).mul_add(t, 6.0).mul_add(t, -4.0).mul_add(t, 1.0)
}

fn descending_fade(position: f32, inverse_width: f32) -> f32 {
    let t = if inverse_width == 0.0 { f32::from(position > 0.0) } else { (position * inverse_width).clamp(0.0, 1.0) };
    (t * t).mul_add(2.0_f32.mul_add(t, -3.0), 1.0)
}
