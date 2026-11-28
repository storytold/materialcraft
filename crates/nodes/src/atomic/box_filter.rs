use craft_texture::{Image, Limits, MAX_SIDE};
use materialcraft_core::OutputFormat;
use rayon::prelude::*;

mod mip_window;

/// Filter on a periodic sheared grid, accounting for the transverse shift at a seam.
pub(super) fn sheared(input: &Image, reach: f32, horizontal: bool, slope: f32, source: OutputFormat, destination: OutputFormat) -> Option<Image> {
    let (width, height) = (input.width(), input.height());
    let (length, across) = if horizontal { (width, height) } else { (height, width) };
    let radius = reach + 0.5;
    if !length.is_power_of_two() || !across.is_power_of_two() || !radius.is_finite() || radius > length as f32 {
        return None;
    }
    // Shearing and reduction accumulate before normalization. Use the bounded fallback for HDR overflow.
    let limit = f32::MAX / (2.0 * radius + 2.0);
    if !input.data().iter().all(|v| v.abs() <= limit) {
        return None;
    }
    let intermediate = if destination == OutputFormat::Inherit { OutputFormat::U16 } else { destination };
    // The slope is in texels. A full period along the line spans this many transverse UV periods.
    let uv_slope = slope * (length as f32 / across as f32);
    let shear = |image: &Image, forward: bool, format| {
        let slope = if forward { uv_slope } else { -uv_slope };
        let translation = slope * 0.5;
        Image::generate_uv(width, height, image.kind(), |u, v| {
            // The horizontal forward pass measures along the row from its far edge.
            // Keep the transform columns in sample order before adding the translation.
            let uv = match (horizontal, forward) {
                (true, true) => [u, (-slope).mul_add(1.0 - u, v) + translation],
                (true, false) => [u, (slope * u + v) - translation],
                (false, _) => [slope.mul_add(v, u) - translation, v],
            };
            crate::texture_sampler::sample(image, uv, [true; 2], format, None)
        })
    };
    let displaced = crate::output_precision::apply(shear(input, true, source), intermediate);
    let filtered = mip_window::filter(&displaced, reach, horizontal, true, intermediate, intermediate, mip_window::Edges::Sheared(uv_slope));
    let filtered = crate::output_precision::apply(filtered, intermediate);
    Some(shear(&filtered, false, intermediate))
}

pub(super) fn short_line(input: &Image, reach: f32, step: [f32; 2], tile: [bool; 2], source_format: OutputFormat) -> Image {
    let center_weight = 0.5 / (0.5 + reach);
    let edge_weight = reach * center_weight;
    // The two passes visit samples in different directions. Cancellation exposes their order.
    let combine = |first: [f32; 4], center: [f32; 4], last: [f32; 4]| {
        std::array::from_fn(|c| last[c].mul_add(edge_weight, center[c].mul_add(center_weight, first[c] * edge_weight)))
    };
    if reach > 1.0 || step.iter().any(|v| v.fract() != 0.0) {
        // A fractional side tap filters at the source storage precision.
        let offset = if reach > 1.0 { 2.0 - 1.0 / reach } else { 1.0 };
        let du = step[0] * offset / input.width() as f32;
        let dv = step[1] * offset / input.height() as f32;
        return Image::generate(input.width(), input.height(), input.kind(), |x, y| {
            let u = (x as f32 + 0.5) / input.width() as f32;
            let v = (y as f32 + 0.5) / input.height() as f32;
            let first = crate::texture_sampler::sample(input, [u - du, v - dv], tile, source_format, None);
            let center = input.get(i64::from(x), i64::from(y), false);
            let last = crate::texture_sampler::sample(input, [u + du, v + dv], tile, source_format, None);
            combine(first, center, last)
        });
    }
    let step = step.map(|v| v as i64);
    Image::generate(input.width(), input.height(), input.kind(), |x, y| {
        let (x, y) = (i64::from(x), i64::from(y));
        let first = input.get_axes(x - step[0], y - step[1], tile);
        let center = input.get_axes(x, y, tile);
        let last = input.get_axes(x + step[0], y + step[1], tile);
        combine(first, center, last)
    })
}

pub(super) fn axis(input: &Image, reach: f32, horizontal: bool, tile: bool, source: OutputFormat, destination: OutputFormat) -> Image {
    if !reach.is_finite() || reach <= 0.0 {
        return input.clone();
    }

    // Bound work for hostile strengths to the largest supported image dimension
    let reach = reach.min(MAX_SIDE as f32 - 0.5);
    let radius = 0.5 + reach;
    let length = if horizontal { input.width() } else { input.height() };
    // Wide windows sum before normalization. Keep the finite HDR and non-finite
    // behavior of the extended-precision path when those intermediate sums can overflow.
    let limit = f32::MAX / (2.0 * radius + 2.0);
    if length.is_power_of_two() && radius <= length as f32 && input.data().iter().all(|v| v.abs() <= limit) {
        return mip_window::filter(input, reach, horizontal, tile, source, destination, mip_window::Edges::Paired);
    }
    let extent = (radius + 0.5).floor() as i64;
    let weight = 1.0 / (2.0 * f64::from(radius));
    let edge_weight = (0.5 - (extent as f64 - 0.5) * weight).max(0.0);
    let (length, lines) = if horizontal { (input.width(), input.height()) } else { (input.height(), input.width()) };
    let channels = input.channels();
    let fill_line = |line: u32, output: &mut [f32]| {
        let get = |position: i64| {
            if horizontal { input.get(position, i64::from(line), tile) } else { input.get(i64::from(line), position, tile) }
        };
        for channel in 0..channels {
            // Count NaN, positive infinity and negative infinity separately
            let mut sum = 0.0_f64;
            let mut non_finite = [0_i64; 3];
            for position in (1 - extent)..extent {
                accumulate(&mut sum, &mut non_finite, get(position)[channel], 1);
            }

            for (position, pixel) in output.chunks_exact_mut(channels).enumerate() {
                let position = position as i64;
                let mut value = window_total(sum, non_finite) * weight;
                if edge_weight > 0.0 {
                    value += (f64::from(get(position - extent)[channel]) + f64::from(get(position + extent)[channel])) * edge_weight;
                }

                pixel[channel] = value as f32;
                accumulate(&mut sum, &mut non_finite, get(position + extent)[channel], 1);
                accumulate(&mut sum, &mut non_finite, get(position - extent + 1)[channel], -1);
            }
        }
    };

    let data = if horizontal {
        let mut data = vec![0.0; input.width() as usize * input.height() as usize * channels];
        data.par_chunks_mut(length as usize * channels).enumerate().for_each(|(line, output)| fill_line(line as u32, output));
        data
    } else {
        let results: Vec<Vec<f32>> = (0..lines)
            .into_par_iter()
            .map(|line| {
                let mut output = vec![0.0; length as usize * channels];
                fill_line(line, &mut output);
                output
            })
            .collect();
        let mut data = Vec::with_capacity(input.width() as usize * input.height() as usize * channels);
        for y in 0..input.height() as usize {
            for line in &results {
                data.extend_from_slice(line.get(y * channels..(y + 1) * channels).unwrap_or(&[]));
            }
        }

        data
    };

    let mut limits = Limits::default();
    limits.max_side = u32::MAX;
    limits.max_pixels = u64::MAX;
    limits.max_bytes = u64::MAX;

    // `data` keeps the size and kind of a valid image, so `from_data` accepts it
    Image::from_data(input.width(), input.height(), input.kind(), data, &limits).unwrap_or_else(|_| input.clone())
}

fn accumulate(sum: &mut f64, non_finite: &mut [i64; 3], value: f32, sign: i64) {
    if value.is_finite() {
        *sum += f64::from(value) * sign as f64;
    } else {
        let index = if value.is_nan() {
            0
        } else if value.is_sign_positive() {
            1
        } else {
            2
        };
        non_finite[index] += sign;
    }
}

fn window_total(sum: f64, non_finite: [i64; 3]) -> f64 {
    // A non-finite sample must stop affecting the running sum once it leaves the window
    if non_finite[0] > 0 || (non_finite[1] > 0 && non_finite[2] > 0) {
        f64::NAN
    } else if non_finite[1] > 0 {
        f64::INFINITY
    } else if non_finite[2] > 0 {
        f64::NEG_INFINITY
    } else {
        sum
    }
}
