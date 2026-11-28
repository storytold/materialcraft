use craft_texture::{Image, Kind};
use materialcraft_core::OutputFormat;

/// Resample one stored input level at destination pixel centers.
pub fn resize_input_level(image: &Image, width: u32, height: u32, format: OutputFormat, tiling: [bool; 2]) -> Image {
    Image::generate_uv(width, height, image.kind(), |u, v| sample(image, [u, v], tiling, format, None))
}

/// Leave each axis within one doubling or halving for the final two-dimensional filter.
pub(crate) fn next_filter_size(current: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let advance = |now: u32, end: u32| {
        if now.saturating_mul(2) < end {
            now.saturating_mul(2)
        } else if end.saturating_mul(2) < now {
            now.div_ceil(2)
        } else {
            now
        }
    };
    let intermediate = (advance(current.0, target.0), advance(current.1, target.1));
    if intermediate == current { target } else { intermediate }
}

/// Resize a raster bound directly to an input, converting its channels after the first sampling pass.
pub fn resize_bound_image(image: Image, size: (u32, u32), kind: Kind, source: OutputFormat, output: OutputFormat, tiling: [bool; 2]) -> Image {
    resize_stored_levels(image, size, kind, source, output, tiling, |current, target| {
        let next = |from: u32, to: u32| if from < to { from.saturating_mul(2).min(to) } else { from.div_ceil(2).max(to) };
        (next(current.0, target.0), next(current.1, target.1))
    })
}

/// A graph connection uses the same final two-axis filter as an atomic node input.
pub fn resize_connected_image(image: Image, size: (u32, u32), kind: Kind, source: OutputFormat, output: OutputFormat, tiling: [bool; 2]) -> Image {
    resize_stored_levels(image, size, kind, source, output, tiling, next_filter_size)
}

fn resize_stored_levels(
    mut image: Image,
    size: (u32, u32),
    kind: Kind,
    mut source: OutputFormat,
    output: OutputFormat,
    tiling: [bool; 2],
    next_size: impl Fn((u32, u32), (u32, u32)) -> (u32, u32),
) -> Image {
    let size = craft_texture::clamp_size(size.0, size.1);
    loop {
        let current = (image.width(), image.height());
        if current != size {
            let (w, h) = next_size(current, size);
            image = resize_input_level(&image, w, h, source, tiling);
        }
        // Later levels read the stored channels, including any RGB-to-gray conversion.
        if image.kind() != kind {
            image = match kind {
                Kind::Gray => {
                    let weight = 1.0 / 3.0;
                    image.map(Kind::Gray, |p| [p[2].mul_add(weight, p[1].mul_add(weight, p[0] * weight)), 0.0, 0.0, 0.0])
                }
                Kind::Color => image.to_color(),
            };
        }
        image = crate::output_precision::apply(image, output);
        if (image.width(), image.height()) == size {
            return image;
        }
        source = output;
    }
}

/// Value and pattern functions filter stored channel levels using full-precision texel fractions.
pub(crate) fn sample_uniform(image: &Image, uv: [f32; 2], format: OutputFormat, bilinear: bool, repeat: bool) -> [f32; 4] {
    if !uv.iter().all(|v| v.is_finite()) {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let offset = if bilinear { 0.5 } else { 0.0 };
    let axis = |v: f32, size: u32| {
        let extent = size as f32;
        let scaled = v * extent;
        if !repeat {
            // Clamp the tap indices. Even four reads of one corner retain their original filter weights.
            let texel = if scaled.is_finite() { scaled - offset } else { scaled.signum() * extent };
            let first = texel.floor();
            return (first.clamp(-1.0, extent) as i64, texel - first);
        }
        // At extreme magnitudes the UV is a whole number of repeats. Keep neighboring indices bounded.
        let texel = if scaled.is_finite() { scaled - offset } else { -offset };
        let first = texel.floor();
        // A nearest read exactly on a negative texel boundary belongs to the preceding texel.
        let previous = !bilinear && texel < 0.0 && texel == first;
        ((first % extent) as i64 - i64::from(previous), texel - first)
    };
    let (x, dx) = axis(uv[0], image.width());
    let (y, dy) = axis(uv[1], image.height());
    let levels = match format {
        OutputFormat::U8 => Some(255.0_f32),
        OutputFormat::U16 => Some(65535.0_f32),
        _ => None,
    };
    let fetch = |x, y| {
        let pixel = image.get(x, y, repeat);
        levels.map_or(pixel, |maximum| pixel.map(|v| (v * maximum).round()))
    };
    let a = fetch(x, y);
    let filtered = if bilinear {
        let b = fetch(x + 1, y);
        let c = fetch(x, y + 1);
        let d = fetch(x + 1, y + 1);
        std::array::from_fn(|i| {
            let top = a[i] * (1.0 - dx) + b[i] * dx;
            let bottom = c[i] * (1.0 - dx) + d[i] * dx;
            top * (1.0 - dy) + bottom * dy
        })
    } else {
        a
    };
    levels.map_or(filtered, |maximum| filtered.map(|v| v * maximum.recip()))
}

/// Bilinear texture filtering. Untiled axes clamp unless the caller supplies a border color.
pub(crate) fn sample(image: &Image, uv: [f32; 2], tiling: [bool; 2], format: OutputFormat, matte: Option<[f32; 4]>) -> [f32; 4] {
    if !uv.iter().all(|v| v.is_finite()) {
        return matte.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    }
    filter_taps(image, taps(image, uv, tiling), tiling, format, matte, std::convert::identity)
}

/// Pixel functions address each coordinate before selecting or filtering texels.
pub(crate) fn sample_pixel(image: &Image, uv: [f32; 2], format: OutputFormat, bilinear: bool, repeat: bool) -> [f32; 4] {
    // A singular projection can invalidate just one axis. Repeats sample the seam,
    // while a clamped positive infinity reaches the far edge.
    let uv = uv.map(|v| {
        if v.is_finite() {
            v
        } else if !repeat && v == f32::INFINITY {
            1.0
        } else {
            0.0
        }
    });
    if !bilinear {
        return image.get((uv[0] * image.width() as f32).floor() as i64, (uv[1] * image.height() as f32).floor() as i64, repeat);
    }
    let tiling = [repeat; 2];
    if format != OutputFormat::F32 {
        return sample(image, uv, tiling, format, None);
    }
    // Filtered F32 reads flush subnormals at the texels and again at the result.
    filter_taps(image, taps(image, uv, tiling), tiling, format, None, |pixel| pixel.map(crate::function::flush_subnormal))
}

/// Average each clamped 2×2 footprint at source precision, then store the reduced level.
pub(crate) fn reduce_level(image: &Image, source: OutputFormat, destination: OutputFormat) -> Image {
    let level = Image::generate(image.width().div_ceil(2), image.height().div_ceil(2), image.kind(), |x, y| {
        let (x, y) = (i64::from(x) * 2, i64::from(y) * 2);
        let taps = [(x, y, 64), (x + 1, y, 64), (x, y + 1, 64), (x + 1, y + 1, 64)];
        filter_taps(image, taps, [false; 2], source, None, std::convert::identity)
    });
    crate::output_precision::apply(level, destination)
}

fn filter_taps(
    image: &Image,
    taps: [(i64, i64, u32); 4],
    tiling: [bool; 2],
    format: OutputFormat,
    matte: Option<[f32; 4]>,
    prepare: impl Fn([f32; 4]) -> [f32; 4],
) -> [f32; 4] {
    if image.integer_sampling() {
        return sample_normalized(image, taps, tiling, matte);
    }
    let read = |x, y| prepare(texel(image, x, y, tiling, matte));
    if let Some((x, y, _)) = taps.iter().find(|(_, _, weight)| *weight == 256) {
        return read(*x, *y);
    }
    let pixels = taps.map(|(x, y, weight)| (read(x, y), weight));
    prepare(std::array::from_fn(|channel| filter_float(&pixels, channel, format)))
}

fn taps(image: &Image, [u, v]: [f32; 2], tiling: [bool; 2]) -> [(i64, i64, u32); 4] {
    let (x, dx) = axis(u, image.width(), tiling[0]);
    let (y, dy) = axis(v, image.height(), tiling[1]);
    // Round the diagonal coefficient first. The four tap weights must still sum to 256.
    let diagonal = (dx * dy + 128) / 256;
    let bottom = dy - diagonal;
    [(x, y, 256 - dx - bottom), (x + 1, y, dx - diagonal), (x, y + 1, bottom), (x + 1, y + 1, diagonal)]
}

fn texel(image: &Image, x: i64, y: i64, tiling: [bool; 2], matte: Option<[f32; 4]>) -> [f32; 4] {
    if let Some(matte) = matte
        && ((!tiling[0] && !(0..i64::from(image.width())).contains(&x)) || (!tiling[1] && !(0..i64::from(image.height())).contains(&y)))
    {
        return matte;
    }
    image.get_axes(x, y, tiling)
}

fn sample_normalized(image: &Image, taps: [(i64, i64, u32); 4], tiling: [bool; 2], matte: Option<[f32; 4]>) -> [f32; 4] {
    let mut sum = [0.0; 4];
    for (x, y, weight) in taps {
        if weight != 0 {
            for (total, channel) in sum.iter_mut().zip(texel(image, x, y, tiling, matte)) {
                // Stored 16-bit levels times the eight-bit weights sum exactly in f32.
                *total += (channel.clamp(0.0, 1.0) * 65535.0).round() * weight as f32;
            }
        }
    }
    sum.map(|value| (value / 256.0).round() / 65535.0)
}

fn filter_float(taps: &[([f32; 4], u32); 4], channel: usize, format: OutputFormat) -> f32 {
    let mut maximum = 0.0_f32;
    for (pixel, weight) in taps {
        if *weight != 0 {
            let value = pixel[channel];
            if !value.is_finite() {
                return taps.iter().filter(|(_, w)| *w != 0).map(|(p, w)| p[channel] * (*w as f32 / 256.0)).sum();
            }
            maximum = maximum.max(value.abs());
        }
    }
    if maximum == 0.0 {
        // A footprint of negative zeros retains its sign. A zero-weight texel cannot change it.
        return if taps.iter().all(|(pixel, weight)| *weight == 0 || pixel[channel].is_sign_negative()) { -0.0 } else { 0.0 };
    }

    // Align participating texels to their largest exponent, retaining four extra significand bits.
    // The fixed-point products fit in 36 bits even for the largest finite float32 inputs.
    let (fraction_bits, smallest_exponent) = if format == OutputFormat::F16 { (10, -24) } else { (23, -149) };
    let exponent = (f64::from(maximum).to_bits() >> 52) as i32 - 1023;
    let unit_exponent = exponent - fraction_bits - 4;
    let scale = f64::from_bits(((1023 - unit_exponent) as u64) << 52);
    let sum: i64 = taps.iter().filter(|(_, w)| *w != 0).map(|(p, w)| (f64::from(p[channel]) * scale) as i64 * i64::from(*w)).sum();
    let magnitude = sum.unsigned_abs();
    if magnitude == 0 {
        return 0.0;
    }
    // Round the weighted sum once at the input storage precision, including ties away from zero.
    let shift = (63 - magnitude.leading_zeros() as i32 - fraction_bits).max(smallest_exponent - (unit_exponent - 8)).max(0);
    if shift >= 63 {
        return 0.0;
    }
    let rounded = if shift == 0 { magnitude } else { ((magnitude + (1 << (shift - 1))) >> shift) << shift };
    let value = (rounded as f64 / (256.0 * scale)) as f32;
    if sum < 0 { -value } else { value }
}

fn axis(uv: f32, size: u32, tile: bool) -> (i64, u32) {
    // Keep the signed fraction until the integer texel is addressed. Adding a repeat in f32 can
    // round a negative coordinate across an eight-bit weight boundary. Reduce before scaling to
    // keep even extreme coordinates bounded, and retain the half-texel offset exactly.
    let uv = if tile { uv.fract() } else { uv.clamp(-1.0, 2.0) };
    let texel = f64::from(uv) * f64::from(size) - 0.5;
    let first = texel.floor();
    (first as i64, ((texel - first) * 256.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use crate::function::{EvaluationContext, EvaluationState, FunctionScope, compile};
    use craft_texture::{Image, Kind};
    use materialcraft_core::{FunctionGraph, Value};
    use std::collections::BTreeMap;

    #[test]
    fn pixel_texture_reads_address_each_nonfinite_coordinate_independently() {
        let field = Image::generate(16, 16, Kind::Color, |x, y| [(x + 1) as f32 / 64.0, (y + 1) as f32 / 64.0, 0.5, 1.0]);
        let images = [Some(field)];
        let filtered_row = 11.3125 / 64.0;
        for (mode, edge, row) in [(0, 0.015625, 0.171875), (1, 0.1328125, filtered_row), (2, 0.015625, 0.171875), (3, 0.015625, filtered_row)] {
            let mut graph = FunctionGraph::default();
            let uv = graph.add("get_float2", &[], Some(Value::Text("uv".into())));
            graph.output = Some(graph.add("samplecol", &[uv], Some(Value::Int2([0, mode]))));
            let program = compile(&graph, 0).unwrap();
            for value in [f32::NEG_INFINITY, f32::NAN, f32::INFINITY] {
                let vars = BTreeMap::from([("uv".into(), Value::Float2([value, 0.67578125]))]);
                let context = EvaluationContext {
                    scope: FunctionScope::PixelProcessor,
                    position: [0.5; 2],
                    size: [16.0; 2],
                    images: &images,
                    image_formats: &[materialcraft_core::OutputFormat::F32],
                    tile: true,
                    variables: &vars,
                };
                let sampled = program.evaluate(&context, &mut EvaluationState::default()).unwrap();
                let edge = if mode >= 2 && value == f32::INFINITY { 0.25 } else { edge };
                assert_eq!(sampled, Value::Float4([edge, row, 0.5, 1.0]), "{value} in mode {mode}");
            }
        }
    }

    #[test]
    fn floating_texture_filter_preserves_negative_zero_when_every_active_tap_has_its_sign() {
        use materialcraft_core::OutputFormat;

        let image = Image::generate(2, 2, Kind::Color, |x, y| [-0.0, if x == 1 { 0.0 } else { -0.0 }, if y == 1 { 0.0 } else { -0.0 }, 0.0]);
        for format in [OutputFormat::F16, OutputFormat::F32] {
            for (position, expected) in [([0.5, 0.5], [-0.0_f32, 0.0, 0.0, 0.0]), ([0.25, 0.5], [-0.0, -0.0, 0.0, 0.0])] {
                let result = super::sample(&image, position, [true; 2], format, None);
                assert_eq!(result.map(f32::to_bits), expected.map(f32::to_bits));
            }
        }
    }

    #[test]
    fn filtered_coordinates_keep_fraction_bits_through_wrap_and_texel_centering() {
        use materialcraft_core::OutputFormat;

        let image = Image::gray(128, 1, |x, _| (x % 2) as f32);
        for (bits, value) in [(0xbdfff801, 0.5), (0xbdffe801, 0.49609375), (0x3b7effff, 0.00390625), (0xbb808001, 0.99609375)] {
            let sample = super::sample(&image, [f32::from_bits(bits), 0.5], [true; 2], OutputFormat::F32, None);
            assert_eq!(sample[0], value, "{bits:08x}");
        }
    }

    #[test]
    fn value_sampler_repeats_and_filters_at_full_coordinate_precision() {
        use materialcraft_core::OutputFormat;

        let colors = [[19., 67., 129., 251.], [85., 222., 17., 9.], [255., 128., 74., 66.], [3., 102., 39., 109.]];
        let bytes = Image::generate(2, 2, Kind::Color, |x, y| colors[(2 * y + x) as usize].map(|n| n / 255.0)).with_integer_sampling(true);
        let floats = Image::generate(2, 2, Kind::Color, |x, y| colors[(2 * y + x) as usize].map(|n| (n - 97.125) / 13.375));
        let images = [Some(bytes), Some(floats)];
        let context = EvaluationContext {
            scope: FunctionScope::ValueProcessor,
            position: [0.0; 2],
            size: [16.0; 2],
            images: &images,
            image_formats: &[OutputFormat::U8, OutputFormat::F32],
            tile: false,
            variables: &BTreeMap::new(),
        };
        for (slot, filtering, expected) in [
            (0, 0, [0x3eaaaaab, 0x3f5edee0, 0x3d888889, 0x3d109091]),
            (0, 1, [0x3e8013a0, 0x3f2bd646, 0x3e0f43a5, 0x3e744cb7]),
            (1, 1, [0xc01f84ad, 0x40b124ef, 0xc093048e, 0xc02da496]),
        ] {
            let mut graph = FunctionGraph::default();
            let uv = graph.add_constant(Value::Float2([-0.317, 1.093]));
            graph.output = Some(graph.add("samplecol", &[uv], Some(Value::Int2([slot, filtering]))));
            let program = compile(&graph, 0).unwrap();
            assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float4(expected.map(f32::from_bits)));
        }
    }

    #[test]
    fn pixel_sampler_rounds_the_four_integer_texel_weights_together() {
        // RGBA identifies the four corners independently. At (1,128)/256 the diagonal weight is halfway to one.
        let image = Image::generate(2, 2, Kind::Color, |x, y| {
            let mut pixel = [0.0; 4];
            pixel[(2 * y + x) as usize] = 1.0;
            pixel
        })
        .with_integer_sampling(true);
        let variables = BTreeMap::new();
        let mut context = EvaluationContext {
            image_formats: &[],
            scope: FunctionScope::PixelProcessor,
            position: [0.25 + 1.0 / 512.0, 0.5],
            size: [2.0; 2],
            images: &[Some(image)],
            tile: true,
            variables: &variables,
        };
        let mut graph = FunctionGraph::default();
        let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
        graph.output = Some(graph.add("samplecol", &[pos], Some(Value::Int2([0, 1]))));
        let program = compile(&graph, 0).unwrap();
        let pixel = |context: &EvaluationContext<'_>| program.evaluate(context, &mut EvaluationState::default()).unwrap();
        let expected = Value::Float4([32768.0, 0.0, 32512.0, 256.0].map(|n| n / 65535.0));
        assert_eq!(pixel(&context), expected);
        context.position[0] -= 1.0;
        assert_eq!(pixel(&context), expected);
        context.tile = false;
        assert_eq!(pixel(&context), expected);
        graph.nodes.last_mut().unwrap().value = Some(Value::Int2([0, 0]));
        let nearest = compile(&graph, 0).unwrap();
        assert_eq!(nearest.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float4([0.0, 0.0, 1.0, 0.0]));
    }

    #[test]
    fn pixel_sampler_uses_the_storage_precision_of_the_selected_slot() {
        use materialcraft_core::OutputFormat;

        let bits = [0x3fd0e000, 0xbfa46000, 0xc02d8000, 0x407f0000];
        let image = Image::gray(2, 2, |x, y| f32::from_bits(bits[(2 * y + x) as usize]));
        let inputs = [Some(image.clone()), Some(image)];
        let variables = BTreeMap::new();
        let context = EvaluationContext {
            scope: FunctionScope::PixelProcessor,
            position: [0.375, 0.4375],
            size: [2.0; 2],
            images: &inputs,
            image_formats: &[OutputFormat::F32, OutputFormat::F16],
            tile: true,
            variables: &variables,
        };
        for (index, expected) in [(0, 0x3e339000), (1, 0x3e33a000)] {
            let mut graph = FunctionGraph::default();
            let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
            graph.output = Some(graph.add("samplelum", &[pos], Some(Value::Int2([index, 1]))));
            let program = compile(&graph, 0).unwrap();
            assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float(f32::from_bits(expected)));
        }
    }
}
