use egui::Color32;
use materialcraft_engine::image::{Image, linear_to_srgb};

/// The hue is listed in degrees around the color wheel
const HUE_DEGREES: f32 = 360.0;
/// Saturation and value are listed as 8-bit levels
const EIGHT_BIT_LEVELS: f32 = 255.0;

/// The values of one pixel, as the Information panel lists them
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PixelReadout {
    /// The pixel, counted from the top-left corner
    pub(super) position: [u32; 2],
    /// The position as fractions of the image
    pub(super) fraction: [f32; 2],
    /// Red, green, blue and alpha as stored; a gray image repeats its value
    pub(super) color: [f32; 4],
    /// Hue as a fraction of the color wheel, saturation and value
    pub(super) hsv: [f32; 3],
    gray: bool,
}

impl PixelReadout {
    /// The pixel `position` of `image`, held inside the image
    pub(super) fn at(image: &Image, position: [u32; 2]) -> PixelReadout {
        let position = [position[0].min(image.w.saturating_sub(1)), position[1].min(image.h.saturating_sub(1))];
        let color = image.get(i64::from(position[0]), i64::from(position[1]), false);
        let fraction = [position[0] as f32 / image.w.max(1) as f32, position[1] as f32 / image.h.max(1) as f32];

        PixelReadout { position, fraction, color, hsv: hsv(color), gray: image.is_gray() }
    }

    /// The pixel under a position given as fractions of the image
    pub(super) fn at_fraction(image: &Image, fraction: [f32; 2]) -> PixelReadout {
        let pixel = |fraction: f32, side: u32| (fraction.clamp(0.0, 1.0) * side as f32) as u32;
        Self::at(image, [pixel(fraction[0], image.w), pixel(fraction[1], image.h)])
    }

    /// The color as the view draws it
    pub(super) fn swatch(&self) -> Color32 {
        let level = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let shown = |value: f32| if self.gray { level(value) } else { level(linear_to_srgb(value)) };
        Color32::from_rgb(shown(self.color[0]), shown(self.color[1]), shown(self.color[2]))
    }

    /// The readout in three columns: the stored channels, then hue, saturation and value, then
    /// the position. `levels` is the largest whole value of the image's format; float formats
    /// have none and list the stored values only
    pub(super) fn columns(&self, levels: Option<u32>) -> [Vec<String>; 3] {
        let channel = |name: &str, value: f32| match levels {
            Some(levels) => format!("{name}: {:>5} / {value:.6}", (value.clamp(0.0, 1.0) * levels as f32).round() as u32),
            None => format!("{name}: {value:>16.6}"),
        };
        let share = |name: &str, fraction: f32, whole: f32| format!("{name}: {:>3} / {fraction:.6}", (fraction * whole).round() as u32);
        let position = |name: &str, axis: usize| format!("{name}: {:>4} / {:.6}", self.position[axis], self.fraction[axis]);
        let [hue, saturation, value] = self.hsv;

        [
            vec![channel("R", self.color[0]), channel("G", self.color[1]), channel("B", self.color[2]), channel("A", self.color[3])],
            vec![share("H", hue, HUE_DEGREES), share("S", saturation, EIGHT_BIT_LEVELS), share("V", value, EIGHT_BIT_LEVELS)],
            vec![position("X", 0), position("Y", 1)],
        ]
    }
}

/// Hue (a fraction of the wheel), saturation and value of a color held within 0..1
fn hsv(color: [f32; 4]) -> [f32; 3] {
    let [red, green, blue] = [color[0], color[1], color[2]].map(|value| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 });
    let value = red.max(green).max(blue);
    let chroma = value - red.min(green).min(blue);
    if chroma <= f32::EPSILON {
        return [0.0, 0.0, value];
    }

    let sector = if value == red {
        ((green - blue) / chroma).rem_euclid(6.0)
    } else if value == green {
        (blue - red) / chroma + 2.0
    } else {
        (red - green) / chroma + 4.0
    };

    [sector / 6.0, chroma / value, value]
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_engine::image::Kind;

    #[test]
    fn a_pixel_lists_its_stored_values_position_and_hsv() {
        let image = Image::generate(4, 2, Kind::Color, |x, y| if (x, y) == (3, 1) { [1.0, 0.5, 0.0, 0.25] } else { [0.0; 4] });

        let readout = PixelReadout::at(&image, [3, 1]);

        assert_eq!(readout.color, [1.0, 0.5, 0.0, 0.25]);
        assert_eq!((readout.position, readout.fraction), ([3, 1], [0.75, 0.5]));
        assert!((readout.hsv[0] - 30.0 / 360.0).abs() < 1e-6, "orange sits at 30 degrees");
        assert_eq!((readout.hsv[1], readout.hsv[2]), (1.0, 1.0));
        assert_eq!(
            readout.columns(Some(255)),
            [
                vec!["R:   255 / 1.000000", "G:   128 / 0.500000", "B:     0 / 0.000000", "A:    64 / 0.250000"],
                vec!["H:  30 / 0.083333", "S: 255 / 1.000000", "V: 255 / 1.000000"],
                vec!["X:    3 / 0.750000", "Y:    1 / 0.500000"],
            ]
        );
        assert_eq!(PixelReadout::at_fraction(&image, [0.8, 0.9]), readout);
    }

    #[test]
    fn a_float_image_lists_only_its_stored_values() {
        let image = Image::filled(1, 1, Kind::Gray, [2.5, 0.0, 0.0, 0.0]);

        let [channels, hsv, position] = PixelReadout::at(&image, [9, 9]).columns(None);

        assert_eq!(channels, ["R:         2.500000", "G:         2.500000", "B:         2.500000", "A:         1.000000"]);
        assert_eq!(hsv[0], "H:   0 / 0.000000");
        assert_eq!(position, ["X:    0 / 0.000000", "Y:    0 / 0.000000"], "a position outside the image is held inside it");
    }

    #[test]
    fn the_swatch_shows_what_the_view_draws() {
        let gray = Image::filled(1, 1, Kind::Gray, [0.5, 0.0, 0.0, 0.0]);
        assert_eq!(PixelReadout::at(&gray, [0, 0]).swatch(), Color32::from_rgb(128, 128, 128));

        let color = Image::filled(1, 1, Kind::Color, [0.5, 0.0, 1.0, 1.0]);
        assert_eq!(PixelReadout::at(&color, [0, 0]).swatch(), Color32::from_rgb(188, 0, 255));
    }
}
