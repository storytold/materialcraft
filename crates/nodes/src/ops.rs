//! Image operations shared by many nodes: Gaussian blur, Euclidean distance transform, slopes,
//! color space helpers and blend modes.

use craft_texture::{Image, Kind, Limits};
use rayon::prelude::*;

/// Gaussian blur with standard deviation `sigma` (pixels). Small sigmas use a sampled kernel;
/// larger ones three box passes (a close Gaussian approximation in O(1) per pixel).
pub fn gaussian(image: &Image, sigma: f32, tile: bool) -> Image {
    const MIN_SIGMA_PIXELS: f32 = 0.05;
    const BOX_BLUR_MIN_SIGMA_PIXELS: f32 = 3.0;
    const KERNEL_RADIUS_SIGMAS: f32 = 3.0;

    if sigma.is_nan() || sigma <= MIN_SIGMA_PIXELS || !sigma.is_finite() {
        return image.clone();
    }

    let sigma = sigma.min(image.width().max(image.height()) as f32);
    if sigma < BOX_BLUR_MIN_SIGMA_PIXELS {
        let radius = (sigma * KERNEL_RADIUS_SIGMAS).ceil() as i64;
        let kernel: Vec<f32> = (-radius..=radius).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let kernel_sum: f32 = kernel.iter().sum();
        let kernel: Vec<f32> = kernel.iter().map(|v| v / kernel_sum).collect();
        let pass = |src: &Image, horizontal: bool| convolve_1d(src, &kernel, radius, horizontal, tile);
        return pass(&pass(image, true), false);
    }

    // Box widths for three passes (Kovesi, "Fast almost-Gaussian filtering")
    let pass_count = 3.0f32;
    let lower_width = ((12.0 * sigma * sigma / pass_count + 1.0).sqrt()).floor() as i64;
    let lower_width = if lower_width % 2 == 0 { lower_width - 1 } else { lower_width };
    let upper_width = lower_width + 2;
    let lower_passes = ((12.0 * sigma * sigma - pass_count * (lower_width * lower_width) as f32 - 4.0 * pass_count * lower_width as f32 - 3.0 * pass_count)
        / (-4.0 * lower_width as f32 - 4.0))
        .round() as i64;
    let mut output = image.clone();
    for i in 0..3 {
        let radius = (if i < lower_passes { lower_width } else { upper_width } - 1) / 2;
        output = box_1d(&box_1d(&output, radius.max(0), true, tile), radius.max(0), false, tile);
    }

    output
}

/// The default limits, raised to admit `src`, so a pass over an image made with raised limits
/// returns an image of the same size.
fn limits_for(src: &Image) -> Limits {
    let mut limits = Limits::default();
    limits.max_side = limits.max_side.max(src.width()).max(src.height());
    limits.max_pixels = limits.max_pixels.max(u64::from(src.width()) * u64::from(src.height()));
    limits.max_bytes = limits.max_bytes.max(size_of_val(src.data()) as u64);
    limits
}

fn convolve_1d(src: &Image, kernel: &[f32], radius: i64, horizontal: bool, tile: bool) -> Image {
    let channels = src.channels();
    Image::try_generate(src.width(), src.height(), src.kind(), &limits_for(src), |x, y| {
        let mut accumulator = [0.0f32; 4];
        for (i, weight) in kernel.iter().enumerate() {
            let offset = i as i64 - radius;
            let pixel = if horizontal { src.get(x as i64 + offset, y as i64, tile) } else { src.get(x as i64, y as i64 + offset, tile) };
            for channel in 0..channels.clamp(1, 4) {
                accumulator[channel] += pixel[channel] * weight;
            }
        }
        if channels == 1 { [accumulator[0], 0.0, 0.0, 0.0] } else { accumulator }
    })
    .unwrap_or_else(|_| src.clone())
}

/// One running-sum box pass of radius `radius`.
fn box_1d(src: &Image, radius: i64, horizontal: bool, tile: bool) -> Image {
    if radius <= 0 {
        return src.clone();
    }

    let (w, h) = (src.width() as usize, src.height() as usize);
    let channels = src.channels();
    let inverse_width = 1.0 / (2 * radius + 1) as f64;
    let mut data = vec![0.0f32; w * h * channels];
    let line_len = if horizontal { w } else { h };
    let lines = if horizontal { h } else { w };

    // f64 keeps finite HDR windows from overflowing the running sum
    let get = |line: usize, i: i64, channel: usize| -> f64 {
        let n = line_len as i64;
        let i = if tile { i.rem_euclid(n) } else { i.clamp(0, n - 1) } as usize;
        let (x, y) = if horizontal { (i, line) } else { (line, i) };
        f64::from(src.data().get((y * w + x) * channels + channel).copied().unwrap_or(0.0))
    };

    let fill_line = |line: usize, output: &mut [f32]| {
        for channel in 0..channels {
            let mut sum: f64 = (-radius..=radius).map(|i| get(line, i, channel)).sum();
            for i in 0..line_len {
                output[i * channels + channel] = (sum * inverse_width) as f32;
                sum += get(line, i as i64 + radius + 1, channel) - get(line, i as i64 - radius, channel);
            }
        }
    };

    if horizontal {
        data.par_chunks_mut(w * channels).enumerate().for_each(|(line, output)| fill_line(line, output));
    } else {
        let results: Vec<Vec<f32>> = (0..lines)
            .into_par_iter()
            .map(|line| {
                let mut output = vec![0.0f32; line_len * channels];
                fill_line(line, &mut output);
                output
            })
            .collect();
        for (line, values) in results.into_iter().enumerate() {
            for i in 0..line_len {
                let destination = (i * w + line) * channels;
                data[destination..destination + channels].copy_from_slice(&values[i * channels..i * channels + channels]);
            }
        }
    }

    // A valid source has matching dimensions and data length
    Image::from_data(src.width(), src.height(), src.kind(), data, &limits_for(src)).unwrap_or_else(|_| src.clone())
}

/// Squared Euclidean distance (pixels) from every pixel to the nearest pixel where `inside` is
/// true (Felzenszwalb–Huttenlocher, exact, linear time). With `tile`, distances wrap.
pub fn distance_sq(w: u32, h: u32, inside: impl Fn(usize) -> bool + Sync, tile: bool) -> Vec<f32> {
    distance_transform::<false>(w, h, inside, tile).0
}

/// The same exact distance transform, also retaining the nearest mask pixel's row-major index.
/// Empty masks have no nearest pixel: their indices are `usize::MAX`.
pub fn distance_sq_with_sites(w: u32, h: u32, inside: impl Fn(usize) -> bool + Sync, tile: bool) -> (Vec<f32>, Vec<usize>) {
    distance_transform::<true>(w, h, inside, tile)
}

const DISTANCE_INF: f32 = 1e20;

fn edt_1d<const SITES: bool>(f: &[f32]) -> (Vec<f32>, Vec<usize>) {
    let n = f.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    let f: Vec<f64> = f.iter().map(|v| *v as f64).collect();
    let mut d = vec![0.0f32; n];

    // Distance-only callers do not allocate or propagate site indices
    let mut sites = if SITES { vec![usize::MAX; n] } else { Vec::new() };
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = -(DISTANCE_INF as f64);
    z[1] = DISTANCE_INF as f64;
    let cross = |q: usize, p: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
    for q in 1..n {
        let mut s = cross(q, v[k]);
        while k > 0 && s <= z[k] {
            k -= 1;
            s = cross(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = DISTANCE_INF as f64;
    }

    k = 0;
    for (q, dq) in d.iter_mut().enumerate() {
        while k + 1 < n && z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *dq = ((q as f64 - p as f64).powi(2) + f[p]) as f32;
        if SITES && f[p] < DISTANCE_INF as f64 {
            sites[q] = p;
        }
    }

    (d, sites)
}

fn distance_transform<const SITES: bool>(w: u32, h: u32, inside: impl Fn(usize) -> bool + Sync, tile: bool) -> (Vec<f32>, Vec<usize>) {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 {
        return (Vec::new(), Vec::new());
    }

    // Tiling: run on a 3x tiled line so distances cross the edges
    let rep = if tile { 3 } else { 1 };

    // Columns. Site indices are vertical coordinates at this stage
    let cols: Vec<_> = (0..w)
        .into_par_iter()
        .map(|x| {
            let f: Vec<f32> = (0..h * rep).map(|y| if inside((y % h) * w + x) { 0.0 } else { DISTANCE_INF }).collect();
            let (d, sites) = edt_1d::<SITES>(&f);
            let off = if tile { h } else { 0 };
            (d[off..off + h].to_vec(), if SITES { sites[off..off + h].to_vec() } else { Vec::new() })
        })
        .collect();

    // Rows. Carry the chosen column's vertical site through the horizontal transform
    let rows: Vec<_> = (0..h)
        .into_par_iter()
        .map(|y| {
            let f: Vec<f32> = (0..w * rep).map(|x| cols[x % w].0[y]).collect();
            let (d, sites) = edt_1d::<SITES>(&f);
            let off = if tile { w } else { 0 };
            let sites = if SITES {
                sites[off..off + w]
                    .iter()
                    .map(|&x| {
                        if x == usize::MAX {
                            usize::MAX
                        } else {
                            let x = x % w;
                            let sy = cols[x].1[y];
                            if sy == usize::MAX { usize::MAX } else { (sy % h) * w + x }
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            (d[off..off + w].to_vec(), sites)
        })
        .collect();
    drop(cols);
    let mut distances = Vec::with_capacity(w * h);
    let mut sites = if SITES { Vec::with_capacity(w * h) } else { Vec::new() };
    for (row, row_sites) in rows {
        distances.extend(row);
        sites.extend(row_sites);
    }

    (distances, sites)
}

/// Slope (dx, dy) of a gray image by central differences, in value units per pixel.
#[inline]
pub fn slope(img: &Image, x: i64, y: i64, tile: bool) -> (f32, f32) {
    let dx = (img.get_gray(x + 1, y, tile) - img.get_gray(x - 1, y, tile)) * 0.5;
    let dy = (img.get_gray(x, y + 1, tile) - img.get_gray(x, y - 1, tile)) * 0.5;
    (dx, dy)
}

pub fn rgb_to_hsl(c: [f32; 4]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) * 0.5;
    if (mx - mn).abs() < 1e-7 {
        return [0.0, 0.0, l];
    }

    let d = mx - mn;
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    [h / 6.0, s, l]
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 {
        return [l, l, l];
    }

    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

pub const BLEND_MODES: &[&str] = &[
    "Copy",
    "Add (Linear dodge)",
    "Subtract",
    "Multiply",
    "Add Sub",
    "Max (Lighten)",
    "Min (Darken)",
    "Switch",
    "Divide",
    "Overlay",
    "Screen",
    "Soft light",
    "Difference",
    "Color dodge",
    "Color burn",
    "Hard light",
];

/// Blend one channel: `b` background, `f` foreground.
#[inline]
pub fn blend_channel(mode: i32, b: f32, f: f32) -> f32 {
    match mode {
        1 => b + f,
        2 => b - f,
        // Multiply starts at positive zero. Its weighted contribution can still be negative zero.
        3 => b.mul_add(f, 0.0),
        4 => b + 2.0 * f - 1.0,
        5 => b.max(f),
        6 => b.min(f),
        8 => {
            // A zero divisor produces this finite HDR value even when the numerator is zero.
            if f == 0.0 { 16_777_216.0 } else { crate::function::pixel_divide(b, f) }
        }
        9 => {
            if b < 0.5 {
                2.0 * b * f
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - f)
            }
        }
        10 => 1.0 - (1.0 - b) * (1.0 - f),
        11 => {
            // W3C soft light
            if f <= 0.5 {
                b - (1.0 - 2.0 * f) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.max(0.0).sqrt() };
                b + (2.0 * f - 1.0) * (d - b)
            }
        }
        12 => (b - f).abs(),
        13 => {
            // W3C color dodge: a black background stays black, even under a white foreground
            if f >= 1.0 { if b > 0.0 { 1.0 } else { 0.0 } } else { (b / (1.0 - f)).min(1.0) }
        }
        14 => {
            // W3C color burn: a white background stays white, even under a black foreground
            if f <= 0.0 { if b < 1.0 { 0.0 } else { 1.0 } } else { 1.0 - ((1.0 - b) / f).min(1.0) }
        }
        15 => {
            if f < 0.5 {
                2.0 * b * f
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - f)
            }
        }
        _ => f,
    }
}

/// Blend `fg` over `bg` with `mode`, weighted by `opacity` (0..1, already including any mask).
/// `blend_alpha`: composite coverage; false preserves background alpha.
#[inline]
pub fn blend_px(mode: i32, bg: [f32; 4], fg: [f32; 4], opacity: f32, color_blending: i32, blend_alpha: bool, clamp: bool) -> [f32; 4] {
    if mode == 7 {
        // Switch interpolates each channel directly, including alpha.
        let mut out = if opacity > 0.0 && opacity < 1.0 {
            // Fractional weights flush subnormals before multiplication, after the first product and after the final add.
            use crate::function::flush_subnormal;
            std::array::from_fn(|k| {
                let foreground = flush_subnormal(flush_subnormal(fg[k]) * opacity);
                flush_subnormal(flush_subnormal(bg[k]).mul_add(1.0 - opacity, foreground))
            })
        } else {
            std::array::from_fn(|k| bg[k].mul_add(1.0 - opacity, fg[k] * opacity))
        };
        if clamp {
            for c in &mut out[..3] {
                *c = c.clamp(0.0, 1.0);
            }
        }
        return out;
    }
    if ((0..=2).contains(&mode) && color_blending == 0) || (matches!(mode, 3..=6 | 8) && color_blending != 2) {
        // The color weights use the complement of transparency. Coverage uses the original alpha.
        let alpha = if color_blending == 1 { 1.0 } else { 1.0 - (1.0 - fg[3]) };
        let source_weight = alpha * opacity;
        let background_weight = (-alpha).mul_add(opacity, 1.0);
        let mut out = bg;
        for k in 0..3 {
            let color = bg[k].mul_add(background_weight, blend_channel(mode, bg[k], fg[k]) * source_weight);
            out[k] = if clamp { color.clamp(0.0, 1.0) } else { color };
        }
        if blend_alpha {
            out[3] = (1.0 - bg[3]).mul_add(fg[3] * opacity, bg[3]);
        }
        return out;
    }

    let t = opacity * fg[3];
    let coverage = bg[3] * (1.0 - t) + t;
    let mut o = [0.0; 4];
    for k in 0..3 {
        let v = blend_channel(mode, bg[k], fg[k]);

        // Weight before adding to preserve integer-boundary rounding
        o[k] = match color_blending {
            1 => bg[k] * (1.0 - opacity) + v * opacity,
            2 if coverage > 0.0 => {
                let v = if clamp { v.clamp(0.0, 1.0) } else { v };
                (bg[k] * bg[3] * (1.0 - t) + fg[k] * t * (1.0 - bg[3]) + v * bg[3] * t) / coverage
            }
            2 => bg[k],
            3 if mode == 0 => bg[k] * (1.0 - t) + fg[k] * opacity,
            _ => bg[k] * (1.0 - t) + v * t,
        };
        if clamp {
            o[k] = o[k].clamp(0.0, 1.0);
        }
    }

    o[3] = if blend_alpha || color_blending == 2 || (color_blending == 3 && mode == 0) { coverage } else { bg[3] };

    o
}

/// A gray image of the output size from per-pixel values, or an error when the size is out of
/// range or `v` does not hold one value per pixel.
pub fn gray_from(w: u32, h: u32, v: Vec<f32>) -> Result<Image, String> {
    Image::from_data(w, h, Kind::Gray, v, &Limits::default()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sub_blend_weights_source_color_and_coverage_separately() {
        let background = [24.0, 5.0, 8.0, 8.0].map(|byte| byte / 255.0);
        let foreground = [231.0, 250.0, 247.0, 8.0].map(|byte| byte / 255.0);
        let result = blend_px(4, background, foreground, 0.5, 0, true, false);
        assert_eq!(result.map(f32::to_bits), [0x3dda_d4cf, 0x3d0e_0b89, 0x3d3c_b8b5, 0x3d3e_bcbb]);
    }

    #[test]
    fn multiply_blend_preserves_the_sign_after_weighting_a_zero_product() {
        let background = [-0.0, 1.0, -0.5, 0.0];
        for (alpha, expected) in [(0.0, [0.0, 1.0, -0.5, 0.0]), (-1.0, [-0.0, 3.0, -1.25, -1.0])] {
            let result = blend_px(3, background, [0.0, -1.0, -0.5, alpha], 1.0, 0, true, false);
            assert_eq!(result.map(f32::to_bits), expected.map(f32::to_bits));
        }
    }

    #[test]
    fn divide_blend_handles_zero_and_small_nonzero_divisors() {
        for numerator in [-8.0, 0.0, 0.5] {
            assert_eq!(blend_channel(8, numerator, 0.0), 16_777_216.0);
            assert_eq!(blend_channel(8, numerator, -0.0), 16_777_216.0);
        }
        assert_eq!(blend_channel(8, -8.0, 1e-10), -80_000_000_000.0);
    }

    #[test]
    fn color_dodge_and_burn_keep_black_and_white_backgrounds() {
        const DODGE: i32 = 13;
        const BURN: i32 = 14;

        // (background, foreground, expected), W3C compositing formulas
        for (b, f, expected) in [(0.0, 1.0, 0.0), (0.0, 0.5, 0.0), (0.5, 1.0, 1.0), (0.25, 0.5, 0.5), (1.0, 0.75, 1.0)] {
            assert_eq!(blend_channel(DODGE, b, f), expected, "dodge b={b} f={f}");
        }

        for (b, f, expected) in [(1.0, 0.0, 1.0), (1.0, 0.5, 1.0), (0.5, 0.0, 0.0), (0.75, 0.5, 0.5), (0.0, 0.25, 0.0)] {
            assert_eq!(blend_channel(BURN, b, f), expected, "burn b={b} f={f}");
        }
    }

    #[test]
    fn blur_keeps_mean_and_tiles() {
        let img = Image::gray(32, 32, |x, y| if (x / 4 + y / 4) % 2 == 0 { 1.0 } else { 0.0 });
        for s in [1.0, 6.0] {
            let b = gaussian(&img, s, true);

            assert!((b.mean()[0] - img.mean()[0]).abs() < 1e-3, "sigma {s}");
        }
        let flat = gaussian(&Image::filled(8, 8, Kind::Color, [0.2, 0.4, 0.6, 1.0]), 4.0, false);

        assert!((flat.get(3, 3, false)[1] - 0.4).abs() < 1e-5);
    }

    #[test]
    fn box_blur_of_large_hdr_values_stays_finite() {
        let value = 0.75 * f32::MAX;
        let blurred = gaussian(&Image::filled(16, 4, Kind::Gray, [value; 4]), 5.0, true);

        for x in 0..16 {
            let v = blurred.get(x, 1, false)[0];

            assert!(v.is_finite() && (v / value - 1.0).abs() < 1e-5, "pixel {x} is {v}");
        }
    }

    #[test]
    fn blur_keeps_the_size_and_result_of_an_image_over_the_default_limits() {
        let w = craft_texture::MAX_SIDE + 1;
        let mut limits = Limits::default();
        limits.max_side = w;
        let values: Vec<f32> = (0..w).map(|x| if (x / 8) % 2 == 0 { 1.0 } else { 0.0 }).collect();
        let Ok(img) = Image::from_data(w, 1, Kind::Gray, values, &limits) else {
            panic!("a {w}x1 image fits the raised limits");
        };

        for sigma in [1.0, 5.0] {
            let blurred = gaussian(&img, sigma, false);

            assert_eq!((blurred.width(), blurred.height()), (w, 1), "sigma {sigma}");
            assert_ne!(blurred.data(), img.data(), "sigma {sigma} left the pixels unchanged");
        }
    }

    #[test]
    fn gray_from_rejects_an_empty_size_and_a_wrong_value_count() {
        assert!(gray_from(0, 4, Vec::new()).is_err());
        assert!(gray_from(2, 2, vec![0.0; 3]).is_err());
        assert_eq!(gray_from(2, 2, vec![0.5; 4]).map(|image| image.mean()[0]), Ok(0.5));
    }

    #[test]
    fn edt_exact() {
        let d = distance_sq(9, 1, |i| i == 4, false);
        assert_eq!(d, vec![16.0, 9.0, 4.0, 1.0, 0.0, 1.0, 4.0, 9.0, 16.0]);
        let t = distance_sq(9, 1, |i| i == 0, true);
        assert_eq!(t[8], 1.0);
    }

    #[test]
    fn edt_sites_match_brute_force() {
        // Exhaust small masks; use deterministic masks and per-site checks for larger ones
        for (w, h) in [(1usize, 1usize), (1, 7), (7, 1), (2, 3), (3, 3), (5, 7), (8, 6)] {
            let cases = if w * h <= 9 { 1usize << (w * h) } else { 128 };
            for case in 0..cases {
                let mut state = case as u32 + 1;
                let mask: Vec<bool> = (0..w * h)
                    .map(|i| {
                        if w * h <= 9 {
                            case & (1 << i) != 0
                        } else {
                            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                            state.is_multiple_of(7)
                        }
                    })
                    .collect();
                for tile in [false, true] {
                    let (d, sites) = distance_sq_with_sites(w as u32, h as u32, |i| mask[i], tile);

                    assert_eq!(d, distance_sq(w as u32, h as u32, |i| mask[i], tile));

                    for i in 0..w * h {
                        let distance = |site: usize| {
                            let dx = (i % w).abs_diff(site % w);
                            let dy = (i / w).abs_diff(site / w);
                            let (dx, dy) = if tile { (dx.min(w - dx), dy.min(h - dy)) } else { (dx, dy) };
                            (dx * dx + dy * dy) as f32
                        };
                        let expected = (0..mask.len()).filter(|&site| mask[site]).map(distance).reduce(f32::min);
                        if let Some(expected) = expected {
                            assert_eq!(d[i], expected, "{w}x{h}, mask {case}, tile {tile}, pixel {i}");
                            assert!(sites[i] < mask.len() && mask[sites[i]]);
                            assert_eq!(distance(sites[i]), expected);
                        } else {
                            assert_eq!(sites[i], usize::MAX);
                            assert!(d[i] >= DISTANCE_INF);
                        }
                    }
                }
            }
        }

        assert!(distance_sq_with_sites(0, 3, |_| true, true).0.is_empty());
        assert!(distance_sq_with_sites(3, 0, |_| true, false).1.is_empty());
    }

    #[test]
    fn hsl_round_trip() {
        let c = [0.8, 0.3, 0.1, 1.0];
        let h = rgb_to_hsl(c);
        let r = hsl_to_rgb(h[0], h[1], h[2]);
        for k in 0..3 {
            assert!((r[k] - c[k]).abs() < 1e-5);
        }
    }
}
