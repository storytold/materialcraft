use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn histogram_scan_non_uniform_definition() -> NodeDefinition {
    node("histogram_scan_non_uniform", "Histogram Scan Non-Uniform", CATEGORY)
        .desc("Performs histogram threshold remapping driven by a spatial position map.")
        .gray_in("input", "Input")
        .primary_input("input")
        .gray_in("position_map", "Position Map")
        .float("contrast", "Contrast", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(cook_histogram_scan_non_uniform)
}

pub(crate) fn histogram_select_definition() -> NodeDefinition {
    node("histogram_select", "Histogram Select", CATEGORY)
        .desc("Isolates and masks a specific histogram range around a central position with width and contrast.")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("position", "Position", 0.5, 0.0, 1.0)
        .float("width", "Range Width", 0.2, 0.01, 1.0)
        .float("contrast", "Contrast", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(cook_histogram_select)
}

pub(crate) fn histogram_shift_definition() -> NodeDefinition {
    node("histogram_shift", "Histogram Shift", CATEGORY)
        .desc("Shifts histogram levels by a positive or negative offset with clamp or wrap.")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("shift", "Shift Offset", 0.0, -1.0, 1.0)
        .choice("mode", "Mode", &["Clamp", "Wrap/Modulo"], 0)
        .gray_out()
        .cook(cook_histogram_shift)
}

pub(crate) fn hdr_range_viewer_definition() -> NodeDefinition {
    node("hdr_range_viewer", "HDR Range Viewer", CATEGORY)
        .desc("Visual diagnostic tool displaying values below 0.0 in blue and above 1.0 in saturated red.")
        .color_in("input", "Input")
        .primary_input("input")
        .color_out()
        .cook(cook_hdr_range_viewer)
}

pub(crate) fn pow_definition() -> NodeDefinition {
    node("pow", "Pow", CATEGORY)
        .desc("Applies an exponential power curve to pixel values: pow(x, exponent).")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("exponent", "Exponent", 1.0, 0.01, 10.0)
        .gray_out()
        .cook(cook_pow)
}

fn cook_histogram_scan_non_uniform(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let pos_map = context.gray_or(1, 0.5);
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let k = 1.0 / (1.0 - contrast * 0.999);

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let val = input.sample_axes(u, v, tiling)[0];
        let pos = pos_map.sample_axes(u, v, tiling)[0];
        let threshold = 1.0 - pos;
        let scanned = ((val - threshold) * k + 0.5).clamp(0.0, 1.0);
        [scanned, 0.0, 0.0, 1.0]
    })])
}

fn cook_histogram_select(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let pos = context.f("position").clamp(0.0, 1.0);
    let width = context.f("width").clamp(0.001, 1.0) * 0.5;
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let k = 1.0 / (1.0 - contrast * 0.999);

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let val = input.sample_axes(u, v, tiling)[0];
        let dist = (val - pos).abs();
        let selected = if dist <= width { 1.0 } else { 1.0 - (dist - width) * k };
        [selected.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_histogram_shift(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let shift = context.f("shift");
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let val = input.sample_axes(u, v, tiling)[0] + shift;
        let out = match mode {
            1 => val.rem_euclid(1.0),
            _ => val.clamp(0.0, 1.0),
        };
        [out, 0.0, 0.0, 1.0]
    })])
}

fn cook_hdr_range_viewer(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let max_val = p[0].max(p[1]).max(p[2]);
        let min_val = p[0].min(p[1]).min(p[2]);

        if min_val < 0.0 {
            // Negative out-of-range: Blue/Cyan
            let neg = (-min_val).min(1.0);
            [0.0, neg * 0.5, neg, 1.0]
        } else if max_val > 1.0 {
            // Over-range HDR: Saturated Yellow/Red
            let over = (max_val - 1.0).min(1.0);
            [1.0, 1.0 - over * 0.8, 0.0, 1.0]
        } else {
            p
        }
    })])
}

fn cook_pow(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let exp = context.f("exponent").clamp(0.001, 20.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let val = input.sample_axes(u, v, tiling)[0].max(0.0);
        [val.powf(exp).clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
