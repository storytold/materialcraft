use craft_texture::{Image, Limits};
use materialcraft_core::OutputFormat;
use std::collections::BTreeMap;

use super::coordinates::PixelPlanes;
use crate::texture_sampler;

pub(super) struct Mipmaps {
    levels: BTreeMap<usize, Vec<Image>>,
    remaining_bytes: u64,
}

impl Mipmaps {
    pub fn new() -> Self {
        Self { levels: BTreeMap::new(), remaining_bytes: Limits::default().max_bytes }
    }

    pub fn image(&mut self, slot: usize, source: &Image, format: OutputFormat, level: usize) -> Result<Image, String> {
        if level == 0 {
            return Ok(source.clone());
        }
        let levels = self.levels.entry(slot).or_default();
        while levels.len() < level {
            let previous = levels.last().unwrap_or(source);
            if previous.width() == 1 && previous.height() == 1 {
                return Ok(previous.clone());
            }
            let texels = u64::from(previous.width().div_ceil(2)) * u64::from(previous.height().div_ceil(2));
            let bytes = texels * previous.channels() as u64 * size_of::<f32>() as u64;
            // Stamps share cached levels. Bound their combined storage across distinct image slots.
            self.remaining_bytes = self.remaining_bytes.checked_sub(bytes).ok_or("pattern mipmaps exceeded their memory budget")?;
            levels.push(texture_sampler::reduce_level(previous, format, format));
        }
        levels.get(level - 1).cloned().ok_or_else(|| "missing pattern mipmap".into())
    }
}

pub(super) struct Sampler<'a> {
    source: &'a Image,
    slot: usize,
    format: OutputFormat,
    filtering: i32,
    fallback_level: usize,
    level: usize,
    image: Image,
    last_quad: Option<[u32; 2]>,
}

impl<'a> Sampler<'a> {
    pub fn new(source: &'a Image, slot: usize, format: OutputFormat, filtering: i32, fallback: [[f32; 2]; 2]) -> Self {
        let fallback_level = if filtering == 0 { mip_level(source, fallback) } else { 0 };
        Self { source, slot, format, filtering, fallback_level, level: 0, image: source.clone(), last_quad: None }
    }

    pub fn start_copy(&mut self) {
        self.last_quad = None;
    }

    pub fn sample(&mut self, local: [f32; 2], pixel: [u32; 2], planes: Option<&PixelPlanes>, mipmaps: &mut Mipmaps) -> Result<[f32; 4], String> {
        let quad = pixel.map(|v| v & !1);
        if self.filtering == 0 && self.last_quad != Some(quad) {
            self.last_quad = Some(quad);
            let level = planes.map_or(self.fallback_level, |p| mip_level(self.source, p.quad_gradients(quad)));
            if level != self.level {
                self.image = mipmaps.image(self.slot, self.source, self.format, level)?;
                self.level = level;
            }
        }
        Ok(sample(&self.image, local, self.format, self.filtering == 2))
    }
}

fn mip_level(image: &Image, gradients: [[f32; 2]; 2]) -> usize {
    let [dx, dy] = gradients.map(|[u, v]| [u * image.width() as f32, v * image.height() as f32].map(truncate_derivative));
    let footprint = |[x, y]: [f32; 2]| {
        let (mantissa, unit) = derivative_parts(x.abs().min(y.abs()));
        x.abs().max(y.abs()) + ((11 * mantissa + 4) >> 5) as f32 * unit
    };
    let [u_sum, u_diff] = diagonal_components(dx[0], dy[0]);
    let [v_sum, v_diff] = diagonal_components(dx[1], dy[1]);
    let diagonal = diagonal_footprint(u_sum, v_sum).max(diagonal_footprint(u_diff, v_diff));
    let bits = footprint(dx).max(footprint(dy)).max(diagonal).max(1.0).to_bits();
    // The first measured half-level boundary is 1 + 53/128 and repeats at each power of two.
    let exponent = (bits >> 23).saturating_sub(127);
    (exponent + u32::from(bits & 0x7f_ffff >= 53 << 16)).min(13) as usize
}

fn truncate_derivative(value: f32) -> f32 {
    f32::from_bits(value.to_bits() & !0x3fff)
}

// The stored derivative has nine fraction bits. The unit belongs to the input exponent.
fn derivative_parts(magnitude: f32) -> (u32, f32) {
    let bits = magnitude.to_bits();
    (512 + ((bits >> 14) & 511), f32::from_bits(bits & 0x7f80_0000) / 512.0)
}

fn diagonal_components(a: f32, b: f32) -> [f32; 2] {
    let exponent = |v: f32| (v.to_bits() >> 23) & 255;
    let common = exponent(a).max(exponent(b));
    // Keep one guard bit below the larger component's nine fraction bits before
    // adding or subtracting. Cancellation can expose that bit in the diagonal.
    let align = |v| match common - exponent(v) {
        0 | 1 => v,
        gap @ 2..=10 => f32::from_bits(v.to_bits() & (u32::MAX << (13 + gap))),
        _ => 0.0,
    };
    let (a, b) = (align(a), align(b));
    [a + b, a - b]
}

fn diagonal_footprint(u: f32, v: f32) -> f32 {
    let major = u.abs().max(v.abs());
    let (_, major_unit) = derivative_parts(major);
    let (minor_mantissa, minor_unit) = derivative_parts(u.abs().min(v.abs()));
    // Scale each component before summing. The major rounds down at the weighted
    // exponent after a quarter input unit of bias. The minor also includes the 11/32 axis weight.
    let major = truncate_derivative(truncate_derivative(major) * (11.0 / 16.0) + major_unit * 0.25);
    let minor = ((121 * minor_mantissa + 32) >> 9) as f32 * minor_unit;
    major + minor
}

fn sample(image: &Image, local: [f32; 2], format: OutputFormat, nearest: bool) -> [f32; 4] {
    let uv = local.map(|axis| axis * 0.5 + 0.5);
    if nearest {
        if uv.iter().all(|v| (0.0..1.0).contains(v)) { image.sample_nearest_axes(uv[0], uv[1], [false; 2]) } else { [0.0; 4] }
    } else {
        texture_sampler::sample(image, uv, [false; 2], format, Some([0.0; 4]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_diagonal_gradients_cross_half_levels_in_both_directions() {
        let image = Image::gray(32, 32, |_, _| 0.0);
        for (texel_steps, expected) in [([[1864, -2248], [1874, 861]], 1), ([[1884, 649], [1884, 649]], 0), ([[1814, -841], [-2192, -1450]], 0)] {
            let gradients = texel_steps.map(|pair| pair.map(|v| v as f32 / (2048.0 * 32.0)));
            assert_eq!(mip_level(&image, gradients), expected, "{texel_steps:?}");
        }
    }
}
