use materialcraft_engine::HistogramChannel;
use materialcraft_engine::image::{Image, Kind, luminance};

use super::histogram_selection::HistogramSelection;

/// Columns of the histogram: one per 8-bit level
pub(super) const BINS: usize = 256;

/// How often each value of one channel occurs in an image
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Histogram {
    /// One series, or red, green and blue for [`HistogramChannel::Rgb`]
    pub(super) series: Vec<[u32; BINS]>,
    pub(super) pixels: u64,
    /// The smallest and the largest counted value, within 0..1
    pub(super) used: [f32; 2],
}

impl Histogram {
    /// Count `channel` of every pixel; a gray image only has its luminance
    pub(super) fn of(image: &Image, channel: HistogramChannel) -> Histogram {
        let channel = if image.kind == Kind::Gray { HistogramChannel::Luminance } else { channel };
        let series_count = if channel == HistogramChannel::Rgb { 3 } else { 1 };
        let mut histogram = Histogram { series: vec![[0; BINS]; series_count], pixels: 0, used: [f32::INFINITY, f32::NEG_INFINITY] };

        for pixel in image.data.chunks_exact(image.channels()) {
            histogram.pixels += 1;
            match (channel, pixel) {
                (HistogramChannel::Luminance, &[red, green, blue, alpha]) => histogram.count(0, luminance([red, green, blue, alpha])),
                (HistogramChannel::Red, &[red, ..]) => histogram.count(0, red),
                (HistogramChannel::Green, &[_, green, ..]) => histogram.count(0, green),
                (HistogramChannel::Blue, &[_, _, blue, _]) => histogram.count(0, blue),
                (HistogramChannel::Alpha, &[.., alpha]) => histogram.count(0, alpha),
                (HistogramChannel::Rgb, &[red, green, blue, _]) => {
                    histogram.count(0, red);
                    histogram.count(1, green);
                    histogram.count(2, blue);
                }
                (_, &[gray, ..]) => histogram.count(0, gray),
                (_, []) => {}
            }
        }

        if histogram.pixels == 0 {
            histogram.used = [0.0, 0.0];
        }
        histogram
    }

    fn count(&mut self, series: usize, value: f32) {
        // Values outside the displayable range count in the first or the last column
        let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        self.used = [self.used[0].min(value), self.used[1].max(value)];

        let column = column(value);
        if let Some(bin) = self.series.get_mut(series).and_then(|bins| bins.get_mut(column)) {
            *bin = bin.saturating_add(1);
        }
    }

    /// The tallest column of any series, at least 1
    pub(super) fn peak(&self) -> u32 {
        self.series.iter().flatten().copied().max().unwrap_or(1).max(1)
    }

    /// The pixels and the tallest column between two fractions of the range; several series are averaged
    pub(super) fn selected(&self, range: [f32; 2]) -> HistogramSelection {
        let (first, last) = (column(range[0].min(range[1])), column(range[0].max(range[1])));
        let mut total = 0_u64;
        let mut largest = 0;
        for bins in &self.series {
            for count in bins.iter().take(last + 1).skip(first) {
                total += u64::from(*count);
                largest = largest.max(*count);
            }
        }

        HistogramSelection { pixels: total / self.series.len().max(1) as u64, largest }
    }
}

/// The column a value in 0..1 falls in
fn column(value: f32) -> usize {
    let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    ((value * (BINS - 1) as f32 + 0.5) as usize).min(BINS - 1)
}

/// A fraction of the range as a level of a `bits` deep image: whole numbers for 8 and 16 bits
pub(super) fn level(fraction: f32, bits: u32) -> String {
    match bits {
        8 => format!("{}", (fraction * 255.0).round() as u32),
        16 => format!("{}", (fraction * 65535.0).round() as u32),
        _ => format!("{fraction:.4}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color() -> Image {
        Image::generate(4, 1, Kind::Color, |x, _| [x as f32 / 3.0, 0.5, 1.0, if x == 0 { 0.0 } else { 1.0 }])
    }

    #[test]
    fn every_pixel_is_counted_once_per_series() {
        for channel in HistogramChannel::ALL {
            let histogram = Histogram::of(&color(), channel);

            assert_eq!(histogram.pixels, 4, "{channel:?}");
            assert_eq!(histogram.series.len(), if channel == HistogramChannel::Rgb { 3 } else { 1 }, "{channel:?}");
            assert!(histogram.series.iter().all(|bins| bins.iter().sum::<u32>() == 4), "{channel:?}");
        }
    }

    #[test]
    fn channels_count_their_own_values_and_report_the_used_range() {
        let red = Histogram::of(&color(), HistogramChannel::Red);
        assert_eq!((red.series[0][0], red.series[0][85], red.series[0][170], red.series[0][255]), (1, 1, 1, 1));
        assert_eq!(red.used, [0.0, 1.0]);

        let green = Histogram::of(&color(), HistogramChannel::Green);
        assert_eq!(green.series[0][128], 4);
        assert_eq!(green.used, [0.5, 0.5]);

        let alpha = Histogram::of(&color(), HistogramChannel::Alpha);
        assert_eq!((alpha.series[0][0], alpha.series[0][255]), (1, 3));
    }

    #[test]
    fn a_gray_image_counts_its_value_for_any_channel_and_guards_bad_values() {
        let gray = Image::gray(3, 1, |x, _| [0.25, f32::NAN, 7.0][x as usize]);

        let histogram = Histogram::of(&gray, HistogramChannel::Alpha);

        assert_eq!((histogram.series[0][64], histogram.series[0][0], histogram.series[0][255]), (1, 1, 1));
        assert_eq!(histogram.used, [0.0, 1.0]);
    }

    #[test]
    fn a_selection_counts_its_pixels_and_its_tallest_column() {
        let histogram = Histogram::of(&color(), HistogramChannel::Red);

        assert_eq!(histogram.selected([0.3, 0.7]), HistogramSelection { pixels: 2, largest: 1 });
        assert_eq!(histogram.selected([1.0, 0.0]), HistogramSelection { pixels: 4, largest: 1 }, "the ends may come in either order");

        let together = Histogram::of(&color(), HistogramChannel::Rgb);
        assert_eq!(together.selected([0.0, 1.0]), HistogramSelection { pixels: 4, largest: 4 }, "three series are averaged");
    }

    #[test]
    fn levels_follow_the_bit_depth() {
        assert_eq!(level(1.0, 8), "255");
        assert_eq!(level(0.5, 16), "32768");
        assert_eq!(level(0.25, 32), "0.2500");
    }
}
