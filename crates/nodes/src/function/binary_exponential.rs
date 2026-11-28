//! Logarithm and exponential for runtime function expressions.

use super::fixed_point::{Quadratic, square_fraction, truncate_float};
use std::{f64::consts::LN_2, sync::LazyLock};

const WIDTH: f64 = 1.0 / 64.0;
const MANTISSA: u32 = 0x7f_ffff;

// Scale each fitted coefficient to integer units of 2^-38 for evaluation.
static LOGARITHMS: LazyLock<[Quadratic; 64]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let start = 1.0 + index as f64 * WIDTH;
        let reciprocal = WIDTH / (start + 0.5 * WIDTH);
        let derivatives = [reciprocal / LN_2, -reciprocal.powi(2) / LN_2, 2.0 * reciprocal.powi(3) / LN_2];
        let mut bin = Quadratic::fit(|t| (start + t * WIDTH).log2(), derivatives, [26, 21, 22]);
        bin.constant = (bin.constant + 3) << 12;
        bin.curvature *= 2;
        bin
    })
});

static EXPONENTIALS: LazyLock<[Quadratic; 64]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let start = index as f64 * WIDTH;
        let mid = (start + 0.5 * WIDTH).exp2();
        let rate = WIDTH * LN_2;
        let derivatives = [mid * rate, mid * rate.powi(2), mid * rate.powi(3)];
        let mut bin = Quadratic::fit(|t| (start + t * WIDTH).exp2(), derivatives, [25, 21, 23]);
        bin.constant = (bin.constant + 2) << 13;
        bin
    })
});

fn interpolate(bins: &[Quadratic; 64], bits: u32) -> i64 {
    let bin = &bins[((bits & MANTISSA) >> 17) as usize];
    let fraction = bits & 0x1_ffff;
    bin.constant + bin.slope * i64::from(fraction) + bin.curvature * i64::from(square_fraction(fraction >> 1))
}

pub(super) fn log2(value: f32) -> f32 {
    if value.abs() < f32::MIN_POSITIVE {
        return f32::NEG_INFINITY;
    }
    if value <= 0.0 || !value.is_finite() || value == 1.0 {
        return value.log2();
    }
    let bits = value.to_bits();
    let exponent = i64::from(bits >> 23) - 127;
    let fixed = (exponent << 38) + interpolate(&LOGARITHMS, bits);
    // A two-bit truncation precedes float normalization. Negative results round one such unit higher.
    let rounded = (fixed + 837 + if fixed < 0 { 4 } else { 0 }) & !3;
    truncate_float(rounded as f64 / 274_877_906_944.0)
}

pub(super) fn ln(value: f32) -> f32 {
    log2(value) * std::f32::consts::LN_2
}

pub(super) fn uniform_ln(value: f32) -> f32 {
    if value.is_nan() || value < f32::MIN_POSITIVE {
        return 0.0;
    }
    let bits = value.to_bits();
    let exponent = (bits >> 23) as i32 - 126;
    let mantissa = f32::from_bits((126 << 23) | (bits & MANTISSA));
    let u = 1.0 - mantissa;
    // Fit the remainder of -ln(1-u) = u(1+uP(u)) over [0, 0.5]. Uniform products and sums round separately.
    let coefficients = [-2.507_298, 1.991_688_4, -0.599_022_4, 0.341_097_44, 0.175_831_17, 0.251_863_93, 0.333_263_6, 0.500_000_9];
    let remainder = coefficients.into_iter().fold(1.761_320_2, |p, c| p * u + c);
    -u * (1.0 + u * remainder) + exponent as f32 * std::f32::consts::LN_2
}

pub(crate) fn signed_pixel_power(value: f32, exponent: f32) -> f32 {
    exp2(log2(value.abs()) * exponent).copysign(value)
}

pub(super) fn exp(value: f32, pixel: bool) -> f32 {
    if value.is_nan() {
        return if pixel { 0.0 } else { value };
    }
    if value < -88.0 {
        return 0.0;
    }
    if value > 89.0 {
        return f32::INFINITY;
    }
    const LOG_HIGH: f32 = std::f32::consts::LN_2;
    const LOG_LOW: f32 = (LN_2 - LOG_HIGH as f64) as f32;
    let exponent = (value * std::f32::consts::LOG2_E).floor();
    let remainder = (-exponent).mul_add(LOG_LOW, (-exponent).mul_add(LOG_HIGH, value));
    // On [0, ln(2)] the same fitted polynomial uses fused pixel operations and separate uniform operations.
    let coefficients = [0.001_304_379_3, 0.008_378_831, 0.041_653_905, 0.166_668_42, 0.499_999_9, 1.0, 1.0];
    let initial = 0.000_275_553_82_f32;
    let significand = if pixel {
        coefficients.into_iter().fold(initial, |p, c| p.mul_add(remainder, c))
    } else {
        coefficients.into_iter().fold(initial, |p, c| p * remainder + c)
    };
    // The scale itself can be 2^128 while the product still fits in float32.
    let scale = f64::from_bits(((exponent as i32 + 1023) as u64) << 52);
    let result = (f64::from(significand) * scale) as f32;
    if result.is_subnormal() { 0.0 } else { result }
}

pub(super) fn power(base: f32, exponent: f32, pixel: bool) -> f32 {
    if !pixel && (!base.is_finite() || !exponent.is_finite()) {
        return base.powf(exponent);
    }
    if base.abs() < f32::MIN_POSITIVE {
        return 0.0;
    }
    let magnitude = |x| exp(exponent * if pixel { ln(x) } else { uniform_ln(x) }, pixel);
    if base > 0.0 {
        return magnitude(base);
    }
    if exponent.fract() != 0.0 {
        return 0.0;
    }
    let result = magnitude(base.abs());
    if exponent.abs() % 2.0 == 1.0 { -result } else { result }
}

pub(super) fn uniform_exp2(value: f32) -> f32 {
    if value.is_nan() || value <= -126.0 {
        return f32::MIN_POSITIVE;
    }
    let value = super::arithmetic::flush_subnormal(value.min(126.0));
    let exponent = value.floor();
    let remainder = (value - exponent) * std::f32::consts::LN_2;
    // The seventh-degree Taylor series for exp(remainder) rounds each product before the addition.
    let coefficients = [1.0 / 720.0, 1.0 / 120.0, 1.0 / 24.0, 1.0 / 6.0, 0.5, 1.0, 1.0];
    let significand = coefficients.into_iter().fold(1.0 / 5040.0, |p, c| p * remainder + c);
    let scale = f32::from_bits(((exponent as i32 + 127) as u32) << 23);
    significand * scale
}

pub(crate) fn exp2(value: f32) -> f32 {
    if value < -126.0 {
        return 0.0;
    }
    if value >= 128.0 || value.is_nan() {
        return value.exp2();
    }
    let mut fixed = (value * 8_388_608.0) as i32;
    // Negative fractional arguments use the one's complement of the fractional magnitude.
    if value < 0.0 && fixed as u32 & MANTISSA != 0 {
        fixed -= 1;
    }
    let exponent = (fixed >> 23) + 126;
    let polynomial = interpolate(&EXPONENTIALS, fixed as u32);
    let significand = ((polynomial + 16_384 - 2_078) >> 15) as u32;
    f32::from_bits(((exponent as u32) << 23) + significand)
}
