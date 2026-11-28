use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node, texture_sampler};
use craft_texture::{Image, Kind, Limits};
use materialcraft_core::{Gradient, OutputFormat};

pub(super) fn definition() -> NodeDefinition {
    node("gradient_map", "Gradient map", ATOMIC)
        .desc("Maps grayscale values to colors along a gradient.")
        .atomic()
        .gray_in("input", "Input")
        .gradient("gradient", "Gradient", Gradient::default())
        .choice("interpolation", "Interpolation", &["Linear", "Smooth"], 0)
        .boolean("color_mode", "Color output", true)
        .any_out()
        .cook(gradient_map)
}

fn gradient_map(context: &CookContext) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let gray = !context.b("color_mode");
    let kind = if gray { Kind::Gray } else { Kind::Color };
    let table = lookup_table(context.gradient("gradient"), context.i("interpolation") == 1, context.format, gray)?;
    let width = table.width() as f32;
    Ok(vec![input.map(kind, |p| {
        let u = (p[0].clamp(0.0, 1.0) * (width - 1.0) + 0.5) / width;
        texture_sampler::sample(&table, [u, 0.5], [false; 2], OutputFormat::F32, None)
    })])
}

fn lookup_table(mut gradient: Gradient, smooth: bool, format: OutputFormat, gray: bool) -> Result<Image, String> {
    let levels = match format {
        OutputFormat::U8 => Some(255.0),
        OutputFormat::Inherit | OutputFormat::U16 => Some(65535.0),
        OutputFormat::F16 | OutputFormat::F32 => None,
    };
    let width = if levels.is_some() { 256 } else { 1024 };
    let last_index = (width - 1) as f32;
    gradient.keys.retain(|key| key.pos.is_finite());
    if gradient.keys.is_empty() {
        gradient = Gradient::default();
    }
    gradient.sort();
    let gray_words = gray && !smooth && matches!(format, OutputFormat::U16 | OutputFormat::Inherit);
    // The first authored stop owns both sides of a shared position.
    gradient.keys.dedup_by(|later, earlier| later.pos == earlier.pos);
    for key in &mut gradient.keys {
        let position = key.pos.clamp(0.0, 1.0);
        key.pos = if levels.is_some() { (position * 65535.0).round() / 257.0 } else { position * last_index };
        if gray {
            let [r, g, b, _] = key.color;
            let level = (r + g + b) / 3.0;
            key.color = [if gray_words { (level.clamp(0.0, 1.0) * 65535.0).round() / 65535.0 } else { level }; 4];
        } else if levels.is_some() {
            // Integer ramp stops store byte colors even when the output uses 16 bits.
            key.color = key.color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0);
        }
    }
    if gray_words {
        // The first stop also owns a position shared after storage at word precision.
        gradient.keys.dedup_by(|later, earlier| later.pos == earlier.pos);
    }

    let mut entries = vec![gradient.keys.last().map_or([0.0; 4], |key| key.color); width];
    if let Some(first) = gradient.keys.first() {
        entries.iter_mut().take(first.pos.ceil() as usize).for_each(|v| *v = first.color);
    }
    for pair in gradient.keys.windows(2) {
        let [left, right] = pair else { continue };
        // Different authored stops can collapse to one position in an integer table.
        if right.pos <= left.pos {
            continue;
        }
        let first = left.pos.ceil() as usize;
        let end = if gray { right.pos.floor() as usize + 1 } else { right.pos.ceil() as usize };
        let Some(segment) = entries.get_mut(first..end).filter(|s| !s.is_empty()) else { continue };
        let step = (right.pos - left.pos).recip();
        let mut t = (first as f32 - left.pos) * step;
        let delta: [f32; 4] = std::array::from_fn(|c| right.color[c] - left.color[c]);
        if smooth {
            let bias = 2.0 * left.midpoint - 1.0;
            for entry in segment {
                let u = t * (1.0 - bias * (1.0 - t));
                *entry = if gray {
                    [left.color[0] + (delta[0] * u) * (u * (3.0 - 2.0 * u)); 4]
                } else {
                    let weight = u * u * (3.0 - 2.0 * u);
                    std::array::from_fn(|c| left.color[c] + delta[c] * weight)
                };
                // Each addition rounds at the table's working precision.
                t += step;
            }
        } else {
            let base: [f32; 4] = std::array::from_fn(|c| left.color[c] + delta[c] * t);
            let slope = delta.map(|d| d * step);
            for (i, entry) in segment.iter_mut().enumerate() {
                *entry = std::array::from_fn(|c| base[c] + slope[c] * i as f32);
            }
        }
    }
    let data: Vec<f32> = entries.into_iter().flatten().map(|v| levels.map_or(v, |max| (v * max).round() / max)).collect();
    Image::from_data(width as u32, 1, Kind::Color, data, &Limits::default())
        .map(|image| image.with_integer_sampling(levels.is_some()))
        .map_err(|err| err.to_string())
}
