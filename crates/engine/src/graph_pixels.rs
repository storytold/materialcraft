//! Display levels of cooked graph images and node thumbnails.

use crate::image::{Image, linear_to_srgb};

/// Quantize a stored channel, including alpha, to its displayed byte.
pub fn stored_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Interpret a graph channel as sRGB or linear light before displaying it.
pub fn display_byte(value: f32, srgb: bool) -> u8 {
    stored_byte(if srgb { value } else { linear_to_srgb(value) })
}

/// Expand the stored graph pixels into display RGBA, preserving alpha.
pub fn rgba8(image: &Image, srgb: bool) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(image.data().len() / image.channels() * 4);
    if let Some(gray) = image.gray_data() {
        for &value in gray {
            let level = display_byte(value, srgb);
            pixels.extend_from_slice(&[level, level, level, 255]);
        }
    } else {
        for &[r, g, b, a] in image.data().as_chunks::<4>().0 {
            pixels.extend_from_slice(&[display_byte(r, srgb), display_byte(g, srgb), display_byte(b, srgb), stored_byte(a)]);
        }
    }
    pixels
}
