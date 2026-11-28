use craft_texture::{Image, Kind, Limits};
use materialcraft_core::OutputFormat;
use std::sync::Arc;

use super::{
    bitmap, blend,
    coordinates::PixelPlanes,
    footprint::Footprint,
    pattern_stamp::{DrawRun, PatternStamp},
    region::Region,
    shape,
};
use crate::CookContext;
use crate::atomic::input_resize::{InputResize, resized_input};
use crate::function::flush_subnormal;

const MAX_SAMPLES: u64 = 256_000_000;
const MAX_STAMP_COPIES: u64 = 1_048_576;
pub(super) const MIN_STAMP_SIZE: f32 = 1e-8;

pub(super) struct RasterWork {
    pub background: Option<Image>,
    pub streaming: bool,
    samples_left: u64,
    copies_left: u64,
    mipmaps: bitmap::Mipmaps,
}

impl RasterWork {
    pub fn new() -> Self {
        Self { background: None, streaming: false, samples_left: MAX_SAMPLES, copies_left: MAX_STAMP_COPIES, mipmaps: bitmap::Mipmaps::new() }
    }
}

pub(super) fn render(
    context: &CookContext,
    groups: &[DrawRun<'_>],
    opacities: &Result<Vec<f32>, String>,
    nonfinite_gray_opacity: bool,
    base_level_only: &[bool],
    runtime_image_selector: bool,
    work: &mut RasterWork,
) -> Result<Image, String> {
    let (width, height) = craft_texture::clamp_size(context.w, context.h);
    let kind = if context.b("color_mode") { Kind::Color } else { Kind::Gray };
    let background = if let Some(previous) = work.background.take() {
        previous
    } else if context.input(0).is_some_and(|image| image.kind() != kind) {
        Image::filled(width, height, kind, [0.0; 4])
    } else {
        resized_input(context, 0, InputResize::PatternBackground).unwrap_or_else(|| {
            let mut color = context.v4("background");
            if kind == Kind::Gray {
                color[0] = (color[0] + color[1] + color[2]) / 3.0;
            }
            Image::filled(width, height, kind, color.map(flush_subnormal))
        })
    };
    if groups.iter().all(DrawRun::is_empty) {
        return Ok(background);
    }

    let channels = kind.channels();
    let opacities = opacities.as_ref().map_err(Clone::clone)?;
    let integer_sampling = background.integer_sampling();
    let mut data = Arc::clone(background.shared_data());
    drop(background);

    let pixels = Arc::make_mut(&mut data);
    let render_region = Region::new(context.v4("render_region"))?;
    let span = render_region.period();
    let pixel_size = [span[0] / width as f32, span[1] / height as f32];
    let axes = context.tiling_axes();
    let cull_outside = context.b("cull_outside");
    let tile_region = Region::tiling(context.v4("tiling_region"), axes, cull_outside)?;

    let mut traversal = groups.iter().flat_map(DrawRun::draws);
    let mut ordered = Vec::new();
    // A runtime image selector reserves the wired slots in port order. Fixed slots acquire their ranks on first use.
    let mut bitmap_ranks = if runtime_image_selector { std::array::from_fn(|index| 256 + index as i32) } else { [0; crate::MAX_IMAGE_INPUTS] };
    let mut next_rank = 256;
    let mut draw_order = |stamp: &PatternStamp| {
        let mut rank = stamp.pattern;
        let mut filtering = 0;
        if rank >= 256
            && let Some(slot) = bitmap_ranks.get_mut((rank - 256) as usize)
        {
            if *slot == 0 {
                *slot = next_rank;
                next_rank += 1;
            }
            rank = *slot;
            filtering = stamp.filtering;
        }
        (rank, filtering, stamp.blend != 0)
    };
    if !traversal.clone().map(|(stamp, _)| draw_order(stamp)).is_sorted() {
        // Assign the remaining ranks in traversal order before the sort compares stamps.
        for draw in traversal.by_ref() {
            draw_order(draw.0);
            ordered.push(draw);
        }
        // Each shape or bitmap slot draws by filter, then additive before the other blend modes.
        ordered.sort_by_key(|(stamp, _)| draw_order(stamp));
    }
    for (stamp, selection) in traversal.chain(ordered) {
        if !stamp.center.iter().chain(&stamp.size).chain([&stamp.rotation, &stamp.variation]).all(|value| value.is_finite()) {
            return Err("pattern parameters must be finite".into());
        }

        // Check the original center before wrapping can move it back inside the tiling region.
        if (cull_outside && !tile_region.contains(stamp.center)) || stamp.size.iter().any(|size| size.abs() < MIN_STAMP_SIZE) {
            continue;
        }

        if !matches!(stamp.blend, 0..=2) {
            return Err(format!("unsupported pattern blending mode {}", stamp.blend));
        }

        if !(2..=18).contains(&stamp.pattern) && !(256..256 + crate::MAX_IMAGE_INPUTS as i32).contains(&stamp.pattern) {
            return Err(format!("unsupported pattern {}", stamp.pattern));
        }

        let opacity = opacities.get(stamp.depth as usize).copied().ok_or("invalid pattern depth")?;
        if !opacity.is_finite() && !nonfinite_gray_opacity {
            return Err("pattern roughness overflowed its depth opacity".into());
        }

        let [red, green, blue, authored_alpha] = stamp.color.map(flush_subnormal);
        // Flush the stored alpha before a large tint could amplify a subnormal product.
        let alpha = flush_subnormal(authored_alpha * flush_subnormal(opacity));
        let channel_tint = [red * alpha, green * alpha, blue * alpha, alpha].map(flush_subnormal);
        let tinted_offset = matches!(stamp.pattern, 17 | 18);
        let sample_shape = shape::prepare(stamp.pattern, stamp.variation);

        let frame = Footprint::new(stamp)?;
        // The horizontal offset ramp still blends its variation at zero or undefined opacity.
        // Image stamps can affect the blend at zero opacity, and skip it at undefined opacity.
        if stamp.pattern != 17 && (authored_alpha.is_nan() || (authored_alpha == 0.0 && stamp.pattern < 256)) {
            continue;
        }
        let mut texture = if stamp.pattern >= 256 {
            let index = (stamp.pattern - 256) as usize + 1;
            let Some(input) = context.input(index).filter(|image| image.kind() == kind) else { continue };
            let format = context.input_formats.get(index).copied().unwrap_or(if input.integer_sampling() { OutputFormat::U16 } else { OutputFormat::F32 });
            let gradients = [
                [frame.cosine * pixel_size[0] / stamp.size[0], -frame.sine * pixel_size[0] / stamp.size[1]],
                [frame.sine * pixel_size[1] / stamp.size[0], frame.cosine * pixel_size[1] / stamp.size[1]],
            ];
            // An automatic read on a texture with only its base level uses bilinear filtering.
            // The original filter selection still determines the order in which the stamps draw.
            let filtering = if stamp.filtering == 0 && base_level_only.get(index) == Some(&true) { 1 } else { stamp.filtering };
            Some(bitmap::Sampler::new(input, index, format, filtering, gradients))
        } else {
            None
        };
        let radius = frame.radius;
        let center = tile_region.copy_origin(stamp.center, axes);
        let repeats = tile_region.repeats(&render_region, center, radius, axes)?;
        let copies = repeats[0].count.checked_mul(repeats[1].count).ok_or("pattern repetition exceeds its budget")?;
        let end = selection.end.min(copies);
        let selected = end.saturating_sub(selection.start);
        if selected == 0 {
            continue;
        }
        // A streamed draw has already paid for its visit and is submitted exactly once.
        // Additional tile copies debit the same allowance across every chunk.
        let charged = if work.streaming { selected - 1 } else { selected };
        work.copies_left = work.copies_left.checked_sub(charged).ok_or("pattern repetition exceeds its budget")?;
        let columns = repeats[0].centers();
        let mut copy = 0_u64;
        'rows: for y_center in repeats[1].centers() {
            for x_center in columns.clone() {
                if copy == end {
                    break 'rows;
                }
                copy += 1;
                if copy <= selection.start {
                    continue;
                }
                let center = [x_center, y_center];
                let bounds = render_region.bounds(center, radius, [width, height]);
                let [[left, right], [top, bottom]] = bounds;
                let samples = u64::from(right.saturating_sub(left)) * u64::from(bottom.saturating_sub(top));
                if samples == 0 {
                    continue;
                }
                work.samples_left = work.samples_left.checked_sub(samples).ok_or("pattern rendering exceeded its pixel budget")?;
                let planes = PixelPlanes::new(&frame, &render_region, center, stamp.size, [width, height]);
                if let Some(sampler) = &mut texture {
                    sampler.start_copy();
                }
                let [[left, right], [top, bottom]] = planes.as_ref().map_or(bounds, |p| p.clip(bounds));
                for y in top..bottom {
                    let row = planes.as_ref().map(|planes| planes.row(y));
                    for x in left..right {
                        let local = if let Some(row) = &row {
                            row(x)
                        } else {
                            let point = render_region.point([x, y], [width, height]);
                            frame.local(point, center, stamp.size)
                        };
                        let Some(local) = local else { continue };

                        let (foreground, stamp_alpha) = if let Some(sampler) = &mut texture {
                            let sample = sampler.sample(local, [x, y], planes.as_ref(), &mut work.mipmaps)?.map(flush_subnormal);
                            if kind == Kind::Gray {
                                // Store coverage before tinting. Its zero then takes the sign of the opacity.
                                let coverage = (sample[0] - 1.0) + 1.0;
                                ([coverage * alpha; 4], 1.0)
                            } else {
                                let coverage = sample[3] * alpha;
                                let image_alpha = if stamp.premultiplied { 1.0 } else { 1.0 - (1.0 - sample[3]) };
                                let mut color = std::array::from_fn(|i| flush_subnormal(sample[i] * image_alpha) * channel_tint[i]);
                                color[3] = coverage;
                                (color, coverage)
                            }
                        } else {
                            let Some((coverage, offset)) = sample_shape(local[0], local[1])? else { continue };
                            let coverage = flush_subnormal(coverage);
                            let offset = flush_subnormal(offset);
                            let color = channel_tint.map(|tint| if tinted_offset { coverage.mul_add(tint, offset) } else { coverage * tint });
                            (color, color[3])
                        };

                        let pixel = (y as usize * width as usize + x as usize) * channels;
                        let target = pixels.get_mut(pixel..pixel + channels).ok_or("invalid pattern pixel")?;
                        let stamp_alpha = flush_subnormal(stamp_alpha);
                        for (channel, target) in target.iter_mut().enumerate() {
                            let value = flush_subnormal(foreground[channel]);
                            let background = flush_subnormal(*target);
                            *target = flush_subnormal(match (kind, stamp.blend) {
                                (Kind::Color, _) => blend::color(background, value, stamp_alpha, stamp.blend, channel),
                                (Kind::Gray, 1 | 2) => background.max(value),
                                (Kind::Gray, _) if value == 0.0 => background,
                                (Kind::Gray, _) => background + value,
                            });
                            if !target.is_finite() {
                                let undefined_channel = target.is_nan()
                                    && (authored_alpha.is_nan() || background.is_nan() || (kind == Kind::Color && stamp.color[channel].is_nan()));
                                if !nonfinite_gray_opacity && !undefined_channel {
                                    return Err("pattern accumulation overflowed".into());
                                }
                                // Each stamp reads the preceding stored result. Only the completed blend saturates.
                                *target = if target.is_nan() { f32::from_bits(0x7fff_ffff) } else { target.clamp(f32::MIN, f32::MAX) };
                            }
                        }
                    }
                }
            }
        }
    }

    Image::from_data(width, height, kind, data, &Limits::default()).map(|image| image.with_integer_sampling(integer_sampling)).map_err(|e| e.to_string())
}

pub(super) fn depth_opacities(roughness: f32, max_depth: u32, opacity: f32) -> Result<Vec<f32>, String> {
    // Match the depth allocation cap to the 4096-node acyclic traversal limit
    if max_depth >= 4096 || !roughness.is_finite() || roughness < 0.0 {
        return Err("pattern roughness must be finite and nonnegative, with depth below 4096".into());
    }

    let count = max_depth + 1;
    // A zero roughness gives every visited depth the full opacity.
    if roughness == 0.0 {
        return Ok(vec![opacity; count as usize]);
    }

    // Round the geometric endpoint before subtracting it, particularly near unit roughness.
    let first_weight = if roughness == 1.0 {
        1.0 / count as f32
    } else {
        let endpoint = roughness.powf(count as f32);
        (1.0 - roughness) / (1.0 - endpoint)
    };

    // Each child inherits its parent's stored opacity, including the global multiplier.
    Ok(std::iter::successors(Some(first_weight * opacity), |parent| Some(parent * roughness)).take(count as usize).collect())
}
