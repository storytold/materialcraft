use craft_texture::{Image, Limits};
use materialcraft_core::OutputFormat;
use std::sync::Arc;

/// Round to the half value used when storing a cooked channel.
pub fn store_half(pixel: f32) -> f32 {
    if !pixel.is_finite() {
        return if pixel.is_nan() { half::f16::from_bits(0x7fff).to_f32() } else { pixel };
    }
    let value = pixel.clamp(-65504.0, 65504.0);
    let rounded = half::f16::from_f32(value);
    if rounded.to_f32().abs() > value.abs() { half::f16::from_bits(rounded.to_bits().saturating_sub(1)).to_f32() } else { rounded.to_f32() }
}

fn store_normalized(pixel: f32, fraction: f64, maximum: f64) -> f32 {
    let value = if pixel.is_nan() { 0.0 } else { f64::from(pixel).clamp(0.0, 1.0) };
    let fixed = (value * fraction).floor() / fraction;
    let integer = (fixed * maximum - 0.5).ceil() as u32;
    integer as f32 / maximum as f32
}

fn normalized_scale(format: OutputFormat) -> Option<(f64, f64)> {
    match format {
        OutputFormat::U8 => Some((4096.0, f64::from(u8::MAX))),
        OutputFormat::Inherit | OutputFormat::U16 => Some((1_048_576.0, f64::from(u16::MAX))),
        OutputFormat::F16 | OutputFormat::F32 => None,
    }
}

/// Store a single border texel at the precision of the image being sampled.
pub(crate) fn store_pixel(pixel: [f32; 4], format: OutputFormat) -> [f32; 4] {
    if let Some((fraction, maximum)) = normalized_scale(format) {
        pixel.map(|v| store_normalized(v, fraction, maximum))
    } else if format == OutputFormat::F16 {
        pixel.map(store_half)
    } else {
        pixel
    }
}

/// Store a cooked image at its destination precision, including intermediate mip levels.
pub fn apply(image: Image, format: OutputFormat) -> Image {
    let integer = integer_sampling(format);
    if format == OutputFormat::F16 {
        // Half-float storage saturates finite overflow and truncates toward zero.
        return with_pixels(image, integer, |pixel| *pixel = store_half(*pixel));
    }

    let Some((fraction, maximum)) = normalized_scale(format) else {
        return image.with_integer_sampling(integer);
    };

    // Integer outputs retain four fractional guard bits before halfway-down rounding.
    with_pixels(image, integer, |pixel| *pixel = store_normalized(*pixel, fraction, maximum))
}

fn integer_sampling(format: OutputFormat) -> bool {
    matches!(format, OutputFormat::Inherit | OutputFormat::U8 | OutputFormat::U16)
}

/// `image` with `convert` applied to every float and the integer-sampling flag set. The buffer is
/// changed in place when no other image shares it, and copied once otherwise.
fn with_pixels(image: Image, integer_sampling: bool, convert: impl Fn(&mut f32)) -> Image {
    let (width, height, kind) = (image.width(), image.height(), image.kind());

    // Limits that admit any valid image, so one made with raised limits keeps its size
    let mut limits = Limits::default();
    limits.max_side = u32::MAX;
    limits.max_pixels = u64::MAX;
    limits.max_bytes = u64::MAX;

    let mut data = Arc::clone(image.shared_data());
    drop(image);

    Arc::make_mut(&mut data).iter_mut().for_each(convert);

    // `data` keeps the size and kind of a valid image, so `from_data` accepts it
    Image::from_data(width, height, kind, data, &limits)
        .unwrap_or_else(|_| Image::filled(width, height, kind, [0.0; 4]))
        .with_integer_sampling(integer_sampling)
}
