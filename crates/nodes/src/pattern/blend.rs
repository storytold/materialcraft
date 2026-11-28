pub(super) fn color(background: f32, source: f32, alpha: f32, mode: i32, channel: usize) -> f32 {
    if mode == 0 {
        // A zero contribution retains the background, including its zero sign.
        if source == 0.0 {
            return background;
        }
        return if channel == 3 { weighted_sum(source, 1.0 - background, background) } else { background + source };
    }
    let remaining = 1.0 - alpha;
    if remaining == 0.0 && source == 0.0 { 0.0 } else { weighted_sum(background, remaining, source) }
}

/// Blend a weighted component and an addend at the precision of the stamp accumulator.
fn weighted_sum(value: f32, weight: f32, addend: f32) -> f32 {
    if !value.is_finite() || !weight.is_finite() || !addend.is_finite() {
        return value.mul_add(weight, addend);
    }
    if addend == 0.0 {
        return value * weight;
    }
    if value == 0.0 || weight == 0.0 {
        return addend;
    }

    let (value_mantissa, value_exponent) = components(value);
    let (weight_mantissa, weight_exponent) = components(weight);
    let (addend_mantissa, addend_exponent) = components(addend);
    let product_exponent = value_exponent + weight_exponent;
    let aligned = product_exponent.max(addend_exponent - 1);
    // Both integers have 46 fraction bits, before normalizing the product.
    // Keep 26 at the common exponent. Any discarded nonzero tail sets the low bit.
    let align = |mantissa: u64, exponent: i32| {
        let shift = (46 - 26 + aligned - exponent).min(63) as u32;
        let high = mantissa >> shift;
        (high | u64::from((high << shift) != mantissa)) as i32
    };
    let product = align(value_mantissa * weight_mantissa, product_exponent);
    let add = align(addend_mantissa << 23, addend_exponent);
    let signed_product = if value.is_sign_negative() == weight.is_sign_negative() { product } else { -product };
    let sum = if addend.is_sign_negative() { signed_product - add } else { signed_product + add };

    // After cancellation removes two or more leading bits, discard the low
    // guard bits at the original operand precision before storing the result.
    let shift = (product_exponent.max(addend_exponent) - aligned) as u32;
    let magnitude = sum.unsigned_abs();
    let stored = if magnitude < (1 << (25 + shift)) { magnitude & !((2 << shift) - 1) } else { magnitude };
    let unit = f64::from_bits(((1023 + aligned - 26) as u64) << 52);
    (f64::from(stored).copysign(f64::from(sum)) * unit) as f32
}

/// Exact 24-bit significand and unbiased exponent of a nonzero finite component.
fn components(value: f32) -> (u64, i32) {
    let bits = value.to_bits();
    let biased = (bits >> 23) & 255;
    if biased == 0 {
        // Widening normalizes every single-precision subnormal exactly.
        let wide = f64::from(value.abs()).to_bits();
        (0x80_0000 | ((wide >> 29) & 0x7f_ffff), (wide >> 52) as i32 - 1023)
    } else {
        (u64::from(0x80_0000 | (bits & 0x7f_ffff)), biased as i32 - 127)
    }
}
