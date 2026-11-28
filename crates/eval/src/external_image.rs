use crate::NodeOutput;
use craft_texture::{Image, Kind, Limits, MAX_SIDE};
use materialcraft_core::{OutputFormat, Package};
use materialcraft_nodes::CookContext;

/// A power-of-two resize after a bitmap's initial conversion leaves only its base texture level.
pub(super) fn base_level_only(image: &Image, size: (u32, u32), kind: Kind, source: OutputFormat, output: OutputFormat) -> bool {
    let axes = [(image.width(), size.0), (image.height(), size.1)];
    if image.kind() != kind || !axes.iter().all(|(a, b)| a.is_power_of_two() && b.is_power_of_two()) {
        return false;
    }
    let [horizontal, vertical] = axes.map(|(a, b)| a.trailing_zeros().abs_diff(b.trailing_zeros()));
    let depth = |format| if format == OutputFormat::Inherit { OutputFormat::U16 } else { format };
    horizontal.max(vertical) > u32::from(depth(source) != depth(output))
}

/// Odd-sized normalized PNG resources use the same stored and sampled levels as bound rasters.
pub(super) fn resize_bitmap(context: &CookContext, package: &Package) -> Option<NodeOutput> {
    let grayscale = !context.b("color_mode");
    if !grayscale && context.bitmap_color_space() == materialcraft_nodes::BitmapColorSpace::Srgb {
        return None;
    }
    let name = context.text("resource");
    let resource = package.resource(name)?;
    let info = materialcraft_core::png_raster_info(resource.bytes.as_slice())?;
    let [width, height] = info.size;
    // Check the header first so ordinary power-of-two bitmaps still request their decoded image only once.
    if info.grayscale != grayscale || (width.is_power_of_two() && height.is_power_of_two()) || (width, height) == (context.w, context.h) {
        return None;
    }
    let image = context.bitmap_resource(name)?;
    let kind = if grayscale { Kind::Gray } else { Kind::Color };
    resize(&image, (context.w, context.h), kind, info.format, context.format)
}

/// Non-power-of-two external rasters filter each axis directly, storing the result between passes.
pub(super) fn resize(image: &Image, size: (u32, u32), kind: Kind, source: OutputFormat, output: OutputFormat) -> Option<NodeOutput> {
    let size = craft_texture::clamp_size(size.0, size.1);
    let (width, height) = (image.width(), image.height());
    if (width.is_power_of_two() && height.is_power_of_two()) || (width, height) == size || width.max(height) > MAX_SIDE || image.kind() != kind {
        return None;
    }
    let input_levels: Option<f32> = match (image.integer_sampling(), source) {
        (true, OutputFormat::U8) => Some(255.0),
        (true, OutputFormat::U16) => Some(65535.0),
        (false, OutputFormat::F32) => None,
        _ => return None,
    };
    let output_levels = match output {
        OutputFormat::U8 => Some(255.0),
        OutputFormat::Inherit | OutputFormat::U16 => Some(65535.0),
        OutputFormat::F32 | OutputFormat::F16 => None,
    };
    // Keep the lower channel depth through both filter passes. A byte raster sent to
    // a word or float output still filters at byte precision.
    let maximum = match (input_levels, output_levels) {
        (Some(source), Some(output)) => Some(source.min(output)),
        (source, output) => source.or(output),
    };
    let mut filtered = match (input_levels, maximum) {
        (Some(source), Some(working)) if source == working => image.map(kind, |pixel| pixel.map(|v| (v * source).round())),
        (_, Some(working)) => dither(image, working, input_levels.is_some())?,
        _ => image.clone(),
    };
    let (w, h) = size;
    let passes = if w <= width { [(true, w), (false, h)] } else { [(false, h), (true, w)] };
    for (horizontal, end) in passes {
        filtered = filter_axis(filtered, horizontal, end, maximum.is_some());
    }
    if output == OutputFormat::F16 {
        return match source {
            OutputFormat::U8 => {
                let texture = filtered.map(kind, |p| p.map(|v| v / 255.0)).with_integer_sampling(true);
                let stored = crate::output_precision::apply(texture.clone(), OutputFormat::F16);
                Some(NodeOutput::BoundBitmap { stored, texture, texture_format: source })
            }
            OutputFormat::U16 => word_to_half(&filtered).map(NodeOutput::Image),
            // Float rasters are filtered at full precision before the half writer diffuses its rounding error.
            _ => diffuse(&filtered, |original, received| {
                let stored = nearest_positive_half(original + received);
                (stored, original - stored)
            })
            .map(NodeOutput::Image),
        };
    }
    let Some(maximum) = maximum else { return Some(NodeOutput::Image(filtered)) };
    let stored = filtered.map(kind, |p| p.map(|v| v / maximum)).with_integer_sampling(output_levels.is_some());
    if input_levels.is_some() && output_levels.is_none_or(|levels| levels > maximum) {
        // A wider bridge can report the same normalized values as the source, yet interpolation
        // still rounds the weighted sum to normalized integer levels.
        let texture = stored.clone().with_integer_sampling(true);
        Some(NodeOutput::BoundBitmap { stored, texture, texture_format: source })
    } else {
        Some(NodeOutput::Image(stored))
    }
}

fn nearest_positive_half(value: f32) -> f32 {
    let value = value.max(0.0);
    let nearest = half::f16::from_f32(value);
    let lower = nearest.to_f32();
    if lower < value {
        let upper = half::f16::from_bits(nearest.to_bits() + 1).to_f32();
        // The bitmap writer rounds a midpoint to the larger half, including at the subnormal boundary.
        if value - lower == upper - value {
            return upper;
        }
    }
    lower
}

fn filter_axis(image: Image, horizontal: bool, end: u32, integer: bool) -> Image {
    let start = if horizontal { image.width() } else { image.height() };
    if start == end {
        return image;
    }
    let scale = f64::from(end) / f64::from(start);
    let filter_scale = scale.min(1.0);
    let radius = 1.0 / filter_scale;
    let taps: Vec<_> = (0..end)
        .map(|index| {
            // On enlargement the mapped origin and half-texel offset round independently.
            let center = if end > start { f64::from(index) / scale + 0.5 / scale } else { (f64::from(index) + 0.5) / scale };
            let first = (center - 0.5 - radius).ceil().max(0.0) as u32;
            let last = ((center - 0.5 + radius).floor() as u32).min(start - 1);
            // Keep the downsampling factor through normalization. Cancelling it changes rounding at close texels.
            let mut weights: Vec<_> = (first..=last).map(|i| filter_scale * (1.0 - (f64::from(i) + 0.5 - center).abs() * filter_scale).max(0.0)).collect();
            let sum: f64 = weights.iter().sum();
            for w in &mut weights {
                *w /= sum;
            }
            (first, weights)
        })
        .collect();
    let (w, h) = if horizontal { (end, image.height()) } else { (image.width(), end) };
    Image::generate(w, h, image.kind(), |x, y| {
        let index = if horizontal { x } else { y };
        let (first, weights) = &taps[index as usize];
        let mut sum = [0.0_f64; 4];
        for (offset, &weight) in weights.iter().enumerate() {
            if weight == 0.0 {
                continue;
            }
            let pos = i64::from(*first) + offset as i64;
            let pixel = if horizontal { image.get(pos, i64::from(y), false) } else { image.get(i64::from(x), pos, false) };
            for (value, channel) in sum.iter_mut().zip(pixel) {
                *value += f64::from(channel) * weight;
            }
        }
        sum.map(|v| if integer { v.round() as f32 } else { v as f32 })
    })
}

fn next_fraction(state: &mut u64) -> f32 {
    *state = (48_271 * *state) % 2_147_483_647;
    (*state as f64 / 2_147_483_647.0) as f32
}

fn word_to_half(image: &Image) -> Option<Image> {
    let mut state = 1_u64;
    diffuse(image, |original, received| {
        let original = f32::from(original as u16);
        let word = (original + received + (next_fraction(&mut state) - 0.5) * 16.0) as u16;
        let (stored, reconstructed) = match word {
            // The low-end conversion has distinct readback and error values at word three.
            0..=2 => (0.0, 0.0),
            3 => (2.0 / 65536.0, 3.0),
            _ => {
                let stored = crate::output_precision::store_half(f32::from(word) / 65535.0);
                (stored, (stored * 65536.0).min(65535.0))
            }
        };
        (stored, original - reconstructed)
    })
}

fn dither(image: &Image, maximum: f32, word_to_byte: bool) -> Option<Image> {
    let mut state = 1_u64;
    let unit = maximum.recip();
    let limit = f32::MAX / (4.0 * maximum);
    diffuse(image, |value, received| {
        let random = next_fraction(&mut state);
        let original = if value.is_finite() { value.clamp(-limit, limit) } else { f32::from(value > 0.0) };
        let stored = if word_to_byte {
            // One full byte level expands to 257 word levels. The noise spans one high-byte step.
            let word = (original * 65535.0).round();
            ((word + received * 257.0 + (random - 0.5) * 256.0) / 256.0).floor()
        } else {
            // The byte writer adds noise in units of 1/256. Diffused error stays in 1/255 units.
            let offset = if maximum == 255.0 { received * unit + (random - 0.5) / 256.0 } else { (received + (random - 0.5)) * unit };
            ((original + offset) * maximum).round()
        }
        .clamp(0.0, maximum);
        (stored, original * maximum - stored)
    })
}

fn diffuse(image: &Image, mut quantize: impl FnMut(f32, f32) -> (f32, f32)) -> Option<Image> {
    let width = usize::try_from(image.width()).ok()?;
    let channels = image.channels();
    let mut samples = Vec::new();
    samples.try_reserve_exact(image.data().len()).ok()?;
    samples.extend_from_slice(image.data());
    let mut errors: [_; 3] = std::array::from_fn(|_| vec![[0.0_f32; 4]; width + 4]);
    for (y, row) in samples.chunks_exact_mut(width * channels).enumerate() {
        let current = y % 3;
        for (x, pixel) in row.chunks_exact_mut(channels).enumerate() {
            let mut error = [0.0; 4];
            for (c, value) in pixel.iter_mut().enumerate() {
                (*value, error[c]) = quantize(*value, errors[current][x + 2][c]);
            }
            // Two extra columns on each end absorb error outside the image. Propagate the
            // original quantization error once, with the three-row Sierra filter.
            for (dy, column, weight) in [
                (0, x + 3, 5),
                (0, x + 4, 3),
                (1, x, 2),
                (1, x + 1, 4),
                (1, x + 2, 5),
                (1, x + 3, 4),
                (1, x + 4, 2),
                (2, x + 1, 2),
                (2, x + 2, 3),
                (2, x + 3, 2),
            ] {
                let dst = &mut errors[(current + dy) % 3][column];
                for (v, e) in dst.iter_mut().zip(error).take(channels) {
                    *v += e * (weight as f32 / 32.0);
                }
            }
        }
        errors[current].fill([0.0; 4]);
    }
    Image::from_data(image.width(), image.height(), image.kind(), samples, &Limits::default()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_float_rasters_keep_exact_byte_levels_and_dither_fractional_levels() {
        let level = 128.0 / 255.0;
        let flat = Image::filled(256, 3, Kind::Color, [level; 4]);
        let out = resize(&flat, (256, 4), Kind::Color, OutputFormat::F32, OutputFormat::U8).unwrap();
        assert!(out.as_image().unwrap().data().iter().all(|&v| v == level));

        let ramp = Image::generate(256, 3, Kind::Color, |x, _| [(51.0 + x as f32 / 256.0) / 255.0; 4]);
        let out = resize(&ramp, (256, 4), Kind::Color, OutputFormat::F32, OutputFormat::U8).unwrap();
        let image = out.as_image().unwrap();
        for (x, y, channel, expected) in [(16, 1, 1, 51), (219, 1, 2, 52), (18, 3, 1, 52), (105, 3, 3, 51)] {
            assert_eq!((image.get(x, y, false)[channel] * 255.0).round() as u8, expected);
        }
    }
}
