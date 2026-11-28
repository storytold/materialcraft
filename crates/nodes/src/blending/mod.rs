use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub const CATEGORY: &str = "Filters/Blending";

pub(crate) fn definition_color_blend() -> NodeDefinition {
    node("color_blend_node", "Color (Blend Node)", CATEGORY)
        .desc("Combines the luminance of the base with the hue and saturation of the blend color.")
        .color_in("foreground", "Foreground (Color)")
        .primary_input("foreground")
        .color_in("background", "Background (Base)")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_color_blend)
}

pub(crate) fn definition_color_burn() -> NodeDefinition {
    node("color_burn", "Color Burn", CATEGORY)
        .desc("Darkens base pixels to reflect the blend color by increasing contrast.")
        .any_in("foreground", "Foreground")
        .primary_input("foreground")
        .any_in("background", "Background")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .any_out()
        .cook(cook_color_burn)
}

pub(crate) fn definition_color_dodge() -> NodeDefinition {
    node("color_dodge", "Color Dodge", CATEGORY)
        .desc("Brightens base pixels to reflect the blend color by decreasing contrast.")
        .any_in("foreground", "Foreground")
        .primary_input("foreground")
        .any_in("background", "Background")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .any_out()
        .cook(cook_color_dodge)
}

pub(crate) fn definition_difference() -> NodeDefinition {
    node("difference", "Difference", CATEGORY)
        .desc("Subtracts the darker color from the lighter color across all channels.")
        .any_in("foreground", "Foreground")
        .primary_input("foreground")
        .any_in("background", "Background")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .any_out()
        .cook(cook_difference)
}

pub(crate) fn definition_dissolve() -> NodeDefinition {
    node("dissolve", "Dissolve", CATEGORY)
        .desc("Randomly scatters foreground pixels onto the background based on opacity and hash noise.")
        .any_in("foreground", "Foreground")
        .primary_input("foreground")
        .any_in("background", "Background")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .any_out()
        .cook(cook_dissolve)
}

pub(crate) fn definition_linear_burn() -> NodeDefinition {
    node("linear_burn", "Linear Burn", CATEGORY)
        .desc("Darkens the base color to reflect the blend color by decreasing the brightness: B + F - 1.")
        .any_in("foreground", "Foreground")
        .primary_input("foreground")
        .any_in("background", "Background")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .any_out()
        .cook(cook_linear_burn)
}

pub(crate) fn definition_luminosity_blend() -> NodeDefinition {
    node("luminosity_blend_node", "Luminosity (Blend Node)", CATEGORY)
        .desc("Combines the hue and saturation of the base with the luminance of the blend color.")
        .color_in("foreground", "Foreground (Luminance)")
        .primary_input("foreground")
        .color_in("background", "Background (Base Color)")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_luminosity_blend)
}

pub(crate) fn definition_switch() -> NodeDefinition {
    node("switch", "Switch", CATEGORY)
        .desc("Selects between two inputs (Input 1 when False, Input 2 when True).")
        .any_in("input1", "Input 1 (False)")
        .primary_input("input1")
        .any_in("input2", "Input 2 (True)")
        .boolean("switch", "Switch Selection", false)
        .any_out()
        .cook(cook_switch)
}

pub(crate) fn definition_multi_switch() -> NodeDefinition {
    node("multi_switch", "Multi Switch", CATEGORY)
        .desc("Selects one stream out of up to 4 input streams via index selector.")
        .any_in("input1", "Input 1")
        .primary_input("input1")
        .any_in("input2", "Input 2")
        .any_in("input3", "Input 3")
        .any_in("input4", "Input 4")
        .choice("selection", "Selection", &["Input 1", "Input 2", "Input 3", "Input 4"], 0)
        .any_out()
        .cook(cook_multi_switch)
}

fn rgb_to_hsl(rgb: [f32; 3]) -> [f32; 3] {
    let r = rgb[0].clamp(0.0, 1.0);
    let g = rgb[1].clamp(0.0, 1.0);
    let b = rgb[2].clamp(0.0, 1.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) * 0.5;
    if delta < 1e-5 {
        [0.0, 0.0, l]
    } else {
        let s = if l < 0.5 { delta / (max + min) } else { delta / (2.0 - max - min) };
        let h = if (max - r).abs() < 1e-5 {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if (max - g).abs() < 1e-5 {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        [h, s, l]
    }
}

fn hsl_to_rgb(hsl: [f32; 3]) -> [f32; 3] {
    let (h, s, l) = (hsl[0], hsl[1], hsl[2]);
    if s < 1e-5 {
        [l, l, l]
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        let hue2rgb = |t: f32| {
            let t = t.rem_euclid(1.0);
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 1.0 / 2.0 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        [hue2rgb(h + 1.0 / 3.0), hue2rgb(h), hue2rgb(h - 1.0 / 3.0)]
    }
}

fn cook_color_blend(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let hsl_f = rgb_to_hsl([f[0], f[1], f[2]]);
        let hsl_b = rgb_to_hsl([b[0], b[1], b[2]]);

        // Color blend: Hue and Saturation from foreground, Luminance from background
        let blended = hsl_to_rgb([hsl_f[0], hsl_f[1], hsl_b[2]]);

        [b[0] * (1.0 - m) + blended[0] * m, b[1] * (1.0 - m) + blended[1] * m, b[2] * (1.0 - m) + blended[2] * m, b[3]]
    })])
}

fn cook_luminosity_blend(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let hsl_f = rgb_to_hsl([f[0], f[1], f[2]]);
        let hsl_b = rgb_to_hsl([b[0], b[1], b[2]]);

        // Luminosity blend: Hue and Saturation from background, Luminance from foreground
        let blended = hsl_to_rgb([hsl_b[0], hsl_b[1], hsl_f[2]]);

        [b[0] * (1.0 - m) + blended[0] * m, b[1] * (1.0 - m) + blended[1] * m, b[2] * (1.0 - m) + blended[2] * m, b[3]]
    })])
}

fn cook_color_burn(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, fg.kind(), |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let burn = |b: f32, f: f32| if f <= 0.0 { 0.0 } else { (1.0 - ((1.0 - b) / f).min(1.0)).max(0.0) };
        [b[0] * (1.0 - m) + burn(b[0], f[0]) * m, b[1] * (1.0 - m) + burn(b[1], f[1]) * m, b[2] * (1.0 - m) + burn(b[2], f[2]) * m, b[3]]
    })])
}

fn cook_color_dodge(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, fg.kind(), |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let dodge = |b: f32, f: f32| if f >= 1.0 { 1.0 } else { (b / (1.0 - f)).min(1.0) };
        [b[0] * (1.0 - m) + dodge(b[0], f[0]) * m, b[1] * (1.0 - m) + dodge(b[1], f[1]) * m, b[2] * (1.0 - m) + dodge(b[2], f[2]) * m, b[3]]
    })])
}

fn cook_difference(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, fg.kind(), |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let diff = |b: f32, f: f32| (b - f).abs();
        [b[0] * (1.0 - m) + diff(b[0], f[0]) * m, b[1] * (1.0 - m) + diff(b[1], f[1]) * m, b[2] * (1.0 - m) + diff(b[2], f[2]) * m, b[3]]
    })])
}

fn cook_dissolve(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, fg.kind(), |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let pixel_hash = ((x as u32).wrapping_mul(0x1f1f1f1f) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ seed) as f32 / 4_294_967_295.0;
        if pixel_hash < m { f } else { b }
    })])
}

fn cook_linear_burn(context: &CookContext<'_>) -> CookResult {
    let fg = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bg = context.any_or(1, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, fg.kind(), |x, y| {
        let (x, y) = (x as i64, y as i64);
        let f = fg.get(x, y, tile);
        let b = bg.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile)) * opacity;

        let l_burn = |b: f32, f: f32| (b + f - 1.0).clamp(0.0, 1.0);
        [b[0] * (1.0 - m) + l_burn(b[0], f[0]) * m, b[1] * (1.0 - m) + l_burn(b[1], f[1]) * m, b[2] * (1.0 - m) + l_burn(b[2], f[2]) * m, b[3]]
    })])
}

fn cook_switch(context: &CookContext<'_>) -> CookResult {
    let sel = context.b("switch");
    if sel { Ok(vec![context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0])]) } else { Ok(vec![context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0])]) }
}

fn cook_multi_switch(context: &CookContext<'_>) -> CookResult {
    let sel = context.i("selection").clamp(0, 3) as usize;
    Ok(vec![context.any_or(sel, Kind::Color, [0.0, 0.0, 0.0, 1.0])])
}

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        definition_color_blend(),
        definition_color_burn(),
        definition_color_dodge(),
        definition_difference(),
        definition_dissolve(),
        definition_linear_burn(),
        definition_luminosity_blend(),
        definition_switch(),
        definition_multi_switch(),
    ]
}
