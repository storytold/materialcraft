//! Runtime sine and cosine for pixel and uniform function scopes.

use super::fixed_point::{Quadratic, square_fraction, truncate_float};
use std::sync::LazyLock;

const QUARTER: u32 = 1 << 23;

// Derive each quadratic from the sine function over one 64th of the quadrant.
static QUARTER_WAVE: LazyLock<[Quadratic; 64]> = LazyLock::new(|| std::array::from_fn(quarter_polynomial));

fn quarter_polynomial(index: usize) -> Quadratic {
    let width = std::f64::consts::PI / 128.0;
    let (sine, cosine) = ((index as f64 + 0.5) * width).sin_cos();
    let derivatives = [width * cosine, -width.powi(2) * sine, -width.powi(3) * cosine];
    let mut polynomial = Quadratic::fit(|t| ((index as f64 + t) * width).sin(), derivatives, [25, 20, 22]);
    polynomial.constant = match index {
        0 => 0,
        // These three measured intervals start one constant unit below the rounded midpoint.
        2 | 18 | 46 => polynomial.constant - 1,
        _ => polynomial.constant,
    };
    polynomial
}

// One turn spans 25 bits. Retaining the product word before the exponent shift
// reproduces the loss of phase resolution at large angles.
fn phase(bits: u32) -> u32 {
    let exponent = ((bits >> 23) & 255) as i32 - 127;
    let mantissa = (bits & 0x7f_ffff) | (1 << 23);
    let word = ((u64::from(mantissa) * 10_680_707) >> 16) as u32;
    let shift = exponent - 8;
    let shifted = if shift >= 0 { word.checked_shl(shift as u32) } else { word.checked_shr((-shift) as u32) };
    shifted.unwrap_or(0) & (4 * QUARTER - 1)
}

pub(super) fn component(angle: f32, cosine: bool, scope: super::FunctionScope, literal: bool) -> f32 {
    if literal {
        return if cosine { angle.cos() } else { angle.sin() };
    }
    if scope == super::FunctionScope::PixelProcessor { pixel_component(angle, cosine) } else { uniform_component(angle, cosine) }
}

pub(crate) fn pixel_sin_cos(angle: f32) -> (f32, f32) {
    (pixel_component(angle, false), pixel_component(angle, true))
}

fn pixel_component(angle: f32, cosine: bool) -> f32 {
    if !angle.is_finite() {
        return f32::NAN;
    }
    let phase = phase(angle.to_bits()) + if cosine { QUARTER } else { 0 };
    let half = phase & (2 * QUARTER - 1);
    let quarter = if half < QUARTER { half } else { 2 * QUARTER - 1 - half };
    let bin = &QUARTER_WAVE[(quarter >> 17) as usize];
    let fraction = quarter & 0x1_ffff;
    let fixed = (bin.constant << 12) + bin.slope * i64::from(fraction) + bin.curvature * i64::from(square_fraction(fraction >> 1));
    // The quarter-wave magnitude truncates to the next representable float below the polynomial.
    let exact = fixed as f64 / 137_438_953_472.0;
    let magnitude = truncate_float(exact);
    if (phase & (2 * QUARTER) != 0) ^ (!cosine && angle.is_sign_negative()) { -magnitude } else { magnitude }
}

fn uniform_component(angle: f32, cosine: bool) -> f32 {
    uniform_turn_component(angle * (std::f32::consts::FRAC_1_PI * 0.5), cosine)
}

pub(crate) fn uniform_sin_cos_turns(turns: f32) -> (f32, f32) {
    (uniform_turn_component(turns, false), uniform_turn_component(turns, true))
}

fn uniform_turn_component(turns: f32, cosine: bool) -> f32 {
    const QUADRANT: f64 = std::f64::consts::FRAC_PI_2;
    const CUBIC: f64 = -QUADRANT * QUADRANT * QUADRANT / 6.0;
    const FIFTH: f64 = -CUBIC * QUADRANT * QUADRANT / 20.0;
    // The measured seventh-order term targets one minus a float epsilon at the quadrant endpoint.
    const SEVENTH: f32 = (1.0 - f32::EPSILON as f64 - QUADRANT - CUBIC - FIFTH) as f32;
    let turns = turns + if cosine { 0.5 } else { 0.25 };
    let x = 4.0 * (turns - turns.round_ties_even()).abs() - 1.0;
    let square = x * x;
    let cube = x * square;
    let fifth = cube * square;
    let seventh = fifth * square;
    // Each odd power and each sum rounds separately.
    x * std::f32::consts::FRAC_PI_2 + cube * CUBIC as f32 + fifth * FIFTH as f32 + seventh * SEVENTH
}
