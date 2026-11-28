use crate::{output_precision, texture_sampler};
use craft_texture::Image;
use materialcraft_core::OutputFormat;

enum Read {
    Texel { level: usize, position: i64 },
    Filtered(f32),
    AcrossSeam { level: usize, along: f32, shift: f32 },
}

struct Tap {
    read: Read,
    weight: f32,
}

#[derive(Clone, Copy)]
pub(super) enum Edges {
    Paired,
    Sheared(f32),
}

impl Tap {
    fn texel(position: i64, width: u32, weight: f32) -> Self {
        Self { read: Read::Texel { level: width.ilog2() as usize, position: position.div_euclid(i64::from(width)) }, weight }
    }

    fn wrap_sheared(&mut self, length: u32, slope: f32) {
        let Read::Texel { level, position } = self.read else { return };
        let count = i64::from(length >> level);
        let turns = position.div_euclid(count);
        if turns != 0 {
            // Crossing a period on the sheared grid shifts the other coordinate.
            // Interpolate the already stored mip at that shift, including for endpoint taps.
            let along = (position.rem_euclid(count) as f32 + 0.5) / count as f32;
            self.read = Read::AcrossSeam { level, along, shift: turns as f32 * slope };
        }
    }
}

fn window(position: u32, length: u32, reach: f32, tile: bool, edges: Edges) -> Vec<Tap> {
    let extent = reach.ceil() as i64;
    let fraction = reach - (extent - 1) as f32;
    let pair_weight = 1.0 + fraction;
    let center = (position as f32 + 0.5) / length as f32;
    let filtered = |offset| Tap { read: Read::Filtered(center + offset / length as f32), weight: pair_weight };
    let mut left = i64::from(position) - extent;
    let mut right = i64::from(position) + extent;
    let mut taps = Vec::new();
    if !tile && left < 0 {
        // Clamped coverage contributes one weighted read of the original border texel.
        taps.push(Tap::texel(0, 1, fraction + (-left - 1) as f32));
        left = 0;
    } else if matches!(edges, Edges::Paired) && left % 2 == 0 {
        taps.push(filtered(-extent as f32 + 1.0 / pair_weight));
        left += 2;
    } else {
        taps.push(Tap::texel(left, 1, fraction));
        left += 1;
    }
    let last = if !tile && right >= i64::from(length) {
        let weight = fraction + (right - i64::from(length)) as f32;
        right = i64::from(length) - 1;
        Tap::texel(right, 1, weight)
    } else if matches!(edges, Edges::Paired) && right % 2 != 0 {
        right -= 2;
        filtered((extent - 1) as f32 + fraction / pair_weight)
    } else {
        right -= 1;
        Tap::texel(right + 1, 1, fraction)
    };
    while left <= right {
        // An aligned run of 2^k texels is a single stored texel at level k.
        let available = (right - left + 1).min(i64::from(length)) as u32;
        let width = 1 << available.ilog2().min(left.trailing_zeros());
        taps.push(Tap::texel(left, width, width as f32));
        left += i64::from(width);
    }
    taps.push(last);
    taps
}

pub(super) fn filter(input: &Image, reach: f32, horizontal: bool, tile: bool, source: OutputFormat, destination: OutputFormat, edges: Edges) -> Image {
    let radius = 0.5 + reach;
    let length = if horizontal { input.width() } else { input.height() };
    let mut levels = vec![input.clone()];
    let mut format = source;
    let mut size = length;
    let max_width = ((radius * 2.0).ceil() as u32).min(length);
    for _ in 0..max_width.ilog2() {
        let Some(previous) = levels.last() else { break };
        size /= 2;
        let (w, h) = if horizontal { (size, input.height()) } else { (input.width(), size) };
        let reduced = texture_sampler::resize_input_level(previous, w, h, format, [true; 2]);
        levels.push(output_precision::apply(reduced, destination));
        format = destination;
    }
    let windows: Vec<_> = (0..length)
        .map(|position| {
            let mut taps = window(position, length, reach, tile, edges);
            if horizontal {
                taps.reverse();
            }
            if let Edges::Sheared(slope) = edges {
                // In the first aligned pair the full texel starts the sum. Its fractional
                // neighbor then contributes one fused multiply-add before the larger mip reads.
                if let [first, second, ..] = taps.as_mut_slice()
                    && let (Read::Texel { level: 0, position: a }, Read::Texel { level: 0, position: b }) = (&first.read, &second.read)
                    && a.div_euclid(2) == b.div_euclid(2)
                {
                    std::mem::swap(first, second);
                }
                if slope.fract() != 0.0 {
                    for tap in &mut taps {
                        tap.wrap_sheared(length, slope);
                    }
                }
            }
            taps
        })
        .collect();
    let scale = 0.5 / radius;
    // On a sheared grid the normalized integer window includes a positive storage offset.
    // A one-level pulse exposes it before the inverse shear averages neighboring texels.
    let storage_offset = match (edges, destination) {
        (Edges::Sheared(_), OutputFormat::U8) => Some(0.5 / 256.0),
        (Edges::Sheared(_), OutputFormat::U16 | OutputFormat::Inherit) => Some(0.4 / 65535.0),
        _ => None,
    };
    Image::generate(input.width(), input.height(), input.kind(), |x, y| {
        let (position, across, other_size) = if horizontal { (x, y, input.height()) } else { (y, x, input.width()) };
        let other_uv = (across as f32 + 0.5) / other_size as f32;
        let read = |tap: &Tap| match tap.read {
            Read::Texel { level, position } => {
                let (x, y) = if horizontal { (position, i64::from(across)) } else { (i64::from(across), position) };
                levels.get(level).unwrap_or(input).get(x, y, true)
            }
            Read::Filtered(uv) => {
                let xy = if horizontal { [uv, other_uv] } else { [other_uv, uv] };
                texture_sampler::sample(input, xy, [tile; 2], source, None)
            }
            Read::AcrossSeam { level, along, shift } => {
                let across = other_uv + shift;
                let xy = if horizontal { [along, across] } else { [across, along] };
                let format = if level == 0 { source } else { destination };
                texture_sampler::sample(levels.get(level).unwrap_or(input), xy, [true; 2], format, None)
            }
        };
        let Some((first, rest)) = windows.get(position as usize).and_then(|taps| taps.split_first()) else {
            return input.get(i64::from(x), i64::from(y), false);
        };
        let mut sum = read(first).map(|v| v * first.weight);
        for tap in rest {
            for (total, v) in sum.iter_mut().zip(read(tap)) {
                *total = v.mul_add(tap.weight, *total);
            }
        }
        match storage_offset {
            Some(offset) => sum.map(|v| v.mul_add(scale, offset)),
            None => sum.map(|v| v * scale),
        }
    })
}
