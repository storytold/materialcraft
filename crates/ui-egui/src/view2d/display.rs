//! The 8-bit pixels the 2D view draws for an image

use materialcraft_engine::View2d;
use materialcraft_engine::image::{Image, Kind, linear_to_srgb};

/// 8-bit RGBA of `image` as the view's settings show it: the chosen channel, and brighter when
/// the values are read as linear light
pub(super) fn display_pixels(image: &Image, view: &View2d) -> Vec<u8> {
    let mut pixels = image.to_rgba8_display();

    if !view.srgb {
        let encoded = encoded_levels();
        for pixel in pixels.chunks_exact_mut(4) {
            for value in pixel.iter_mut().take(3) {
                *value = encoded[usize::from(*value)];
            }
        }
    }

    let Some(channel) = isolated_channel(&view.channel).filter(|_| image.kind == Kind::Color) else { return pixels };

    for pixel in pixels.chunks_exact_mut(4) {
        let value = pixel[channel];
        pixel.copy_from_slice(&[value, value, value, u8::MAX]);
    }
    pixels
}

/// The index of the single channel `channel` names; `None` for all of them
pub(super) fn isolated_channel(channel: &str) -> Option<usize> {
    match channel {
        "r" => Some(0),
        "g" => Some(1),
        "b" => Some(2),
        "a" => Some(3),
        _ => None,
    }
}

/// Every 8-bit level read as linear light and encoded for the screen
fn encoded_levels() -> [u8; 256] {
    let mut levels = [0; 256];
    for (level, encoded) in levels.iter_mut().enumerate() {
        *encoded = (linear_to_srgb(level as f32 / 255.0).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color() -> Image {
        Image::filled(2, 1, Kind::Color, [0.2, 0.4, 0.6, 0.5])
    }

    #[test]
    fn a_single_channel_is_shown_as_opaque_gray() {
        let all = display_pixels(&color(), &View2d::default());

        for (channel, index) in [("r", 0), ("g", 1), ("b", 2), ("a", 3)] {
            let pixels = display_pixels(&color(), &View2d { channel: channel.into(), ..View2d::default() });
            assert_eq!(pixels[..4], [all[index], all[index], all[index], 255], "{channel}");
        }
    }

    #[test]
    fn values_read_as_linear_are_shown_brighter_and_alpha_is_untouched() {
        let encoded = display_pixels(&color(), &View2d::default());
        let linear = display_pixels(&color(), &View2d { srgb: false, ..View2d::default() });

        assert!((0..3).all(|channel| linear[channel] > encoded[channel]), "{linear:?} {encoded:?}");
        assert_eq!(linear[3], encoded[3]);

        let gray = Image::filled(1, 1, Kind::Gray, [0.5, 0.0, 0.0, 0.0]);
        assert_eq!(display_pixels(&gray, &View2d::default())[0], 128);
        assert_eq!(display_pixels(&gray, &View2d { srgb: false, ..View2d::default() })[0], 188);
    }

    #[test]
    fn a_gray_image_ignores_the_channel_choice() {
        let gray = Image::filled(1, 1, Kind::Gray, [0.25, 0.0, 0.0, 0.0]);

        assert_eq!(display_pixels(&gray, &View2d { channel: "a".into(), ..View2d::default() }), display_pixels(&gray, &View2d::default()));
    }
}
