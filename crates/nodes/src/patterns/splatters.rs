use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn splatter_definition() -> NodeDefinition {
    node("splatter", "Splatter", CATEGORY)
        .desc("Scatters input pattern randomly with position, scale, and angle jitter.")
        .color_in("input", "Pattern")
        .primary_input("input")
        .int("count", "Count", 32, 1, 256)
        .float("size", "Size", 0.15, 0.01, 1.0)
        .float("size_random", "Size Random", 0.5, 0.0, 1.0)
        .float("angle_random", "Angle Random", 1.0, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .color_out()
        .cook(cook_splatter)
}

pub(crate) fn shape_splatter_definition() -> NodeDefinition {
    node("shape_splatter", "Shape Splatter", CATEGORY)
        .desc("Advanced multi-shape scattered distributor with collision, scaling, and luminance random.")
        .color_in("pattern", "Pattern")
        .primary_input("pattern")
        .int("count", "Count", 48, 1, 512)
        .float("size", "Size", 0.12, 0.01, 1.0)
        .float("size_random", "Size Random", 0.4, 0.0, 1.0)
        .float("luminance_random", "Luminance Random", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .color_out()
        .cook(cook_shape_splatter)
}

pub(crate) fn shape_splatter_blend_definition() -> NodeDefinition {
    node("shape_splatter_blend", "Shape Splatter Blend", CATEGORY)
        .desc("Blends multiple shape splatter layer inputs by height or maximum intensity.")
        .color_in("layer_1", "Layer 1")
        .primary_input("layer_1")
        .color_in("layer_2", "Layer 2")
        .choice("mode", "Blend Mode", &["Max (Height)", "Alpha Over", "Add"], 0)
        .color_out()
        .cook(cook_shape_splatter_blend)
}

pub(crate) fn shape_splatter_data_extract_definition() -> NodeDefinition {
    node("shape_splatter_data_extract", "Shape Splatter Data Extract", CATEGORY)
        .desc("Extracts luminance mask, height channel, or alpha from shape splatter output.")
        .color_in("input", "Splatter Input")
        .primary_input("input")
        .choice("extract", "Extract Channel", &["Luminance (Grayscale)", "Alpha Mask", "Color (RGB)"], 0)
        .color_out()
        .cook(cook_shape_splatter_data_extract)
}

pub(crate) fn shape_splatter_to_mask_definition() -> NodeDefinition {
    node("shape_splatter_to_mask", "Shape Splatter to Mask", CATEGORY)
        .desc("Binarizes shape splatter coverage into an occupancy silhouette mask.")
        .color_in("input", "Splatter Input")
        .primary_input("input")
        .float("threshold", "Threshold", 0.1, 0.0, 1.0)
        .gray_out()
        .cook(cook_shape_splatter_to_mask)
}

fn cook_splatter(context: &CookContext<'_>) -> CookResult {
    let pattern = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let count = context.i("count").clamp(1, 512) as usize;
    let base_size = context.f("size").clamp(0.005, 2.0);
    let size_rand = context.f("size_random").clamp(0.0, 1.0);
    let angle_rand = context.f("angle_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let (w, h) = (context.w, context.h);
    let mut out = vec![[0.0f32; 4]; (w * h) as usize];

    for i in 0..count {
        let h0 = (i as u32).wrapping_mul(0x85eb_ca6b) ^ seed;
        let h1 = crate::random::hash(h0);
        let h2 = crate::random::hash(h1);
        let h3 = crate::random::hash(h2);
        let h4 = crate::random::hash(h3);

        let cx = h1 as f32 / 4_294_967_295.0;
        let cy = h2 as f32 / 4_294_967_295.0;
        let size = base_size * (1.0 - (h3 as f32 / 4_294_967_295.0) * size_rand);
        let angle = (h4 as f32 / 4_294_967_295.0) * 2.0 * PI * angle_rand;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let half_px = (size * 0.5 * w as f32).ceil() as i32;
        let min_x = ((cx * w as f32) as i32 - half_px).max(0);
        let max_x = ((cx * w as f32) as i32 + half_px).min(w as i32 - 1);
        let min_y = ((cy * h as f32) as i32 - half_px).max(0);
        let max_y = ((cy * h as f32) as i32 + half_px).min(h as i32 - 1);

        for py in min_y..=max_y {
            let v = (py as f32 + 0.5) / h as f32;
            let dy = v - cy;
            for px in min_x..=max_x {
                let u = (px as f32 + 0.5) / w as f32;
                let dx = u - cx;

                let rx = dx * cos_a - dy * sin_a;
                let ry = dx * sin_a + dy * cos_a;

                let lu = rx / size + 0.5;
                let lv = ry / size + 0.5;

                if (0.0..=1.0).contains(&lu) && (0.0..=1.0).contains(&lv) {
                    let sample = pattern.sample_axes(lu, lv, [false, false]);
                    let idx = (py as usize) * (w as usize) + (px as usize);
                    let dst = out[idx];
                    out[idx] = [dst[0].max(sample[0]), dst[1].max(sample[1]), dst[2].max(sample[2]), dst[3].max(sample[3])];
                }
            }
        }
    }

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Color, out.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_shape_splatter(context: &CookContext<'_>) -> CookResult {
    let pattern = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let count = context.i("count").clamp(1, 512) as usize;
    let base_size = context.f("size").clamp(0.005, 2.0);
    let size_rand = context.f("size_random").clamp(0.0, 1.0);
    let lum_rand = context.f("luminance_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let (w, h) = (context.w, context.h);
    let mut out = vec![[0.0f32; 4]; (w * h) as usize];

    for i in 0..count {
        let h0 = (i as u32).wrapping_mul(0x85eb_ca6b) ^ seed;
        let h1 = crate::random::hash(h0);
        let h2 = crate::random::hash(h1);
        let h3 = crate::random::hash(h2);
        let h4 = crate::random::hash(h3);

        let cx = h1 as f32 / 4_294_967_295.0;
        let cy = h2 as f32 / 4_294_967_295.0;
        let size = base_size * (1.0 - (h3 as f32 / 4_294_967_295.0) * size_rand);
        let lum_mul = 1.0 - (h4 as f32 / 4_294_967_295.0) * lum_rand;

        let half_px = (size * 0.5 * w as f32).ceil() as i32;
        let min_x = ((cx * w as f32) as i32 - half_px).max(0);
        let max_x = ((cx * w as f32) as i32 + half_px).min(w as i32 - 1);
        let min_y = ((cy * h as f32) as i32 - half_px).max(0);
        let max_y = ((cy * h as f32) as i32 + half_px).min(h as i32 - 1);

        for py in min_y..=max_y {
            let v = (py as f32 + 0.5) / h as f32;
            let dy = v - cy;
            for px in min_x..=max_x {
                let u = (px as f32 + 0.5) / w as f32;
                let dx = u - cx;

                let lu = dx / size + 0.5;
                let lv = dy / size + 0.5;

                if (0.0..=1.0).contains(&lu) && (0.0..=1.0).contains(&lv) {
                    let sample = pattern.sample_axes(lu, lv, [false, false]);
                    let idx = (py as usize) * (w as usize) + (px as usize);
                    let dst = out[idx];
                    out[idx] = [dst[0].max(sample[0] * lum_mul), dst[1].max(sample[1] * lum_mul), dst[2].max(sample[2] * lum_mul), dst[3].max(sample[3])];
                }
            }
        }
    }

    let limits = craft_texture::Limits::default();
    let img = Image::from_data(w, h, Kind::Color, out.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?;
    Ok(vec![img])
}

fn cook_shape_splatter_blend(context: &CookContext<'_>) -> CookResult {
    let l1 = context.any_or(0, Kind::Color, [0.0; 4]);
    let l2 = context.any_or(1, Kind::Color, [0.0; 4]);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p1 = l1.sample_axes(u, v, tiling);
        let p2 = l2.sample_axes(u, v, tiling);

        match mode {
            // Alpha Over
            1 => {
                let a = p2[3];
                [p2[0] * a + p1[0] * (1.0 - a), p2[1] * a + p1[1] * (1.0 - a), p2[2] * a + p1[2] * (1.0 - a), p1[3].max(p2[3])]
            }
            // Add
            2 => [(p1[0] + p2[0]).clamp(0.0, 1.0), (p1[1] + p2[1]).clamp(0.0, 1.0), (p1[2] + p2[2]).clamp(0.0, 1.0), p1[3].max(p2[3])],
            // Max (default)
            _ => [p1[0].max(p2[0]), p1[1].max(p2[1]), p1[2].max(p2[2]), p1[3].max(p2[3])],
        }
    })])
}

fn cook_shape_splatter_data_extract(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let extract = context.i("extract");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        match extract {
            // Alpha Mask
            1 => [p[3], p[3], p[3], 1.0],
            // Color RGB
            2 => [p[0], p[1], p[2], 1.0],
            // Luminance
            _ => {
                let lum = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                [lum, lum, lum, 1.0]
            }
        }
    })])
}

fn cook_shape_splatter_to_mask(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let th = context.f("threshold").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let max_val = p[0].max(p[1]).max(p[2]).max(p[3]);
        let mask = if max_val >= th { 1.0 } else { 0.0 };
        [mask, 0.0, 0.0, 1.0]
    })])
}
