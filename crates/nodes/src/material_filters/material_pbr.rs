use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition_material_height_blend() -> NodeDefinition {
    node("material_height_blend", "Material Height Blend", CATEGORY)
        .desc("Blends two complete PBR materials based on elevation height differences and contrast.")
        .color_in("top_albedo", "Top Base Color")
        .primary_input("top_albedo")
        .color_in("top_normal", "Top Normal")
        .gray_in("top_roughness", "Top Roughness")
        .gray_in("top_metallic", "Top Metallic")
        .gray_in("top_height", "Top Height")
        .color_in("bottom_albedo", "Bottom Base Color")
        .color_in("bottom_normal", "Bottom Normal")
        .gray_in("bottom_roughness", "Bottom Roughness")
        .gray_in("bottom_metallic", "Bottom Metallic")
        .gray_in("bottom_height", "Bottom Height")
        .gray_in("mask", "Mask")
        .float("height_offset", "Height Offset", 0.5, 0.0, 1.0)
        .float("contrast", "Transition Contrast", 0.5, 0.0, 1.0)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("roughness", "Roughness")
        .named_gray_out("metallic", "Metallic")
        .named_gray_out("height", "Height")
        .named_gray_out("mask", "Transition Mask")
        .cook(cook_material_height_blend)
}

pub(crate) fn definition_material_color_blend() -> NodeDefinition {
    node("material_color_blend", "Material Color Blend", CATEGORY)
        .desc("Recolors or tints the diffuse/albedo of a PBR material set while maintaining roughness, metallic and normal fidelity.")
        .color_in("base_color", "Base Color")
        .primary_input("base_color")
        .color_in("tint_color", "Tint / Target Color")
        .gray_in("mask", "Mask")
        .choice("blend_mode", "Blend Mode", &["Multiply", "Overlay", "Colorize", "Replace"], 0)
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_material_color_blend)
}

pub(crate) fn definition_material_adjustment_blend() -> NodeDefinition {
    node("material_adjustment_blend", "Material Adjustment Blend", CATEGORY)
        .desc("Applies global exposure, contrast, roughness bias and normal strength to a full PBR material stack.")
        .color_in("base_color", "Base Color")
        .primary_input("base_color")
        .color_in("normal", "Normal")
        .gray_in("roughness", "Roughness")
        .float("exposure", "Albedo Exposure", 0.0, -3.0, 3.0)
        .float("roughness_bias", "Roughness Bias", 0.0, -1.0, 1.0)
        .float("normal_factor", "Normal Intensity Factor", 1.0, 0.0, 5.0)
        .named_color_out("base_color", "Adjusted Base Color")
        .named_color_out("normal", "Adjusted Normal")
        .named_gray_out("roughness", "Adjusted Roughness")
        .cook(cook_material_adjustment_blend)
}

pub(crate) fn definition_material_switch() -> NodeDefinition {
    node("material_switch", "Material Switch", CATEGORY)
        .desc("Switches between two complete PBR material streams via a binary selector.")
        .color_in("mat1_albedo", "Mat 1 Albedo")
        .primary_input("mat1_albedo")
        .color_in("mat1_normal", "Mat 1 Normal")
        .gray_in("mat1_roughness", "Mat 1 Roughness")
        .color_in("mat2_albedo", "Mat 2 Albedo")
        .color_in("mat2_normal", "Mat 2 Normal")
        .gray_in("mat2_roughness", "Mat 2 Roughness")
        .boolean("select_mat2", "Select Material 2", false)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("roughness", "Roughness")
        .cook(cook_material_switch)
}

pub(crate) fn definition_multi_material_blend() -> NodeDefinition {
    node("multi_material_blend", "Multi-Material Blend", CATEGORY)
        .desc("Blends up to 3 material layers sequentially using individual alpha/mask channels.")
        .color_in("base_mat_albedo", "Base Albedo")
        .primary_input("base_mat_albedo")
        .color_in("layer1_albedo", "Layer 1 Albedo")
        .gray_in("layer1_mask", "Layer 1 Mask")
        .color_in("layer2_albedo", "Layer 2 Albedo")
        .gray_in("layer2_mask", "Layer 2 Mask")
        .color_out()
        .cook(cook_multi_material_blend)
}

fn cook_material_height_blend(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let top_bc = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let top_norm = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let top_rough = context.any_or(2, Kind::Gray, [0.5; 4]);
    let top_metal = context.any_or(3, Kind::Gray, [0.0; 4]);
    let top_height = context.any_or(4, Kind::Gray, [0.5; 4]);

    let bot_bc = context.any_or(5, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let bot_norm = context.any_or(6, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let bot_rough = context.any_or(7, Kind::Gray, [0.5; 4]);
    let bot_metal = context.any_or(8, Kind::Gray, [0.0; 4]);
    let bot_height = context.any_or(9, Kind::Gray, [0.0; 4]);

    let mask = context.sized(10).map(|m| m.to_gray());
    let offset = context.f("height_offset").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let slope = 1.0 / (1.0 - contrast * 0.99);
    let tile = context.tile;

    let total = (w * h) as usize;
    let mut out_bc = vec![[0.0f32; 4]; total];
    let mut out_norm = vec![[0.5f32, 0.5, 1.0, 1.0]; total];
    let mut out_rough = vec![0.5f32; total];
    let mut out_metal = vec![0.0f32; total];
    let mut out_height = vec![0.0f32; total];
    let mut out_mask = vec![0.0f32; total];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;

            let ht = top_height.get_gray(xi, yi, tile);
            let hb = bot_height.get_gray(xi, yi, tile);
            let user_m = mask.as_ref().map_or(1.0, |m| m.get_gray(xi, yi, tile));

            let diff = (ht + (offset * 2.0 - 1.0)) - hb;
            let raw_t = diff * 0.5 + 0.5;
            let t = ((raw_t - 0.5) * slope + 0.5).clamp(0.0, 1.0) * user_m;

            let c_top = top_bc.get(xi, yi, tile);
            let c_bot = bot_bc.get(xi, yi, tile);
            out_bc[idx] = [c_bot[0] * (1.0 - t) + c_top[0] * t, c_bot[1] * (1.0 - t) + c_top[1] * t, c_bot[2] * (1.0 - t) + c_top[2] * t, 1.0];

            let n_top = top_norm.get(xi, yi, tile);
            let n_bot = bot_norm.get(xi, yi, tile);
            out_norm[idx] = [n_bot[0] * (1.0 - t) + n_top[0] * t, n_bot[1] * (1.0 - t) + n_top[1] * t, n_bot[2] * (1.0 - t) + n_top[2] * t, 1.0];

            out_rough[idx] = bot_rough.get_gray(xi, yi, tile) * (1.0 - t) + top_rough.get_gray(xi, yi, tile) * t;
            out_metal[idx] = bot_metal.get_gray(xi, yi, tile) * (1.0 - t) + top_metal.get_gray(xi, yi, tile) * t;
            out_height[idx] = hb * (1.0 - t) + ht * t;
            out_mask[idx] = t;
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(w, h, Kind::Color, out_bc.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Color, out_norm.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_rough, &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_metal, &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_height, &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_mask, &limits).map_err(|e| e.to_string())?,
    ])
}

fn cook_material_color_blend(context: &CookContext<'_>) -> CookResult {
    let bc = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let tint = context.any_or(1, Kind::Color, [1.0, 0.5, 0.2, 1.0]);
    let mask = context.sized(2).map(|m| m.to_gray());
    let mode = context.i("blend_mode");
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let base = bc.get(x, y, tile);
        let target = tint.get(x, y, tile);
        let m = mask.as_ref().map_or(1.0, |img| img.get_gray(x, y, tile)) * opacity;

        let blended = match mode {
            1 => [
                if base[0] < 0.5 { 2.0 * base[0] * target[0] } else { 1.0 - 2.0 * (1.0 - base[0]) * (1.0 - target[0]) },
                if base[1] < 0.5 { 2.0 * base[1] * target[1] } else { 1.0 - 2.0 * (1.0 - base[1]) * (1.0 - target[1]) },
                if base[2] < 0.5 { 2.0 * base[2] * target[2] } else { 1.0 - 2.0 * (1.0 - base[2]) * (1.0 - target[2]) },
            ],
            2 => {
                let lum = 0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2];
                [target[0] * lum, target[1] * lum, target[2] * lum]
            }
            3 => [target[0], target[1], target[2]],
            _ => [base[0] * target[0], base[1] * target[1], base[2] * target[2]],
        };

        [
            (base[0] * (1.0 - m) + blended[0] * m).clamp(0.0, 1.0),
            (base[1] * (1.0 - m) + blended[1] * m).clamp(0.0, 1.0),
            (base[2] * (1.0 - m) + blended[2] * m).clamp(0.0, 1.0),
            base[3],
        ]
    })])
}

fn cook_material_adjustment_blend(context: &CookContext<'_>) -> CookResult {
    let bc = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let norm = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let rough = context.any_or(2, Kind::Gray, [0.5; 4]);
    let exp_scale = 2.0f32.powf(context.f("exposure"));
    let r_bias = context.f("roughness_bias");
    let n_factor = context.f("normal_factor").max(0.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let out_bc = Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let c = bc.get(x, y, tile);
        [(c[0] * exp_scale).clamp(0.0, 1.0), (c[1] * exp_scale).clamp(0.0, 1.0), (c[2] * exp_scale).clamp(0.0, 1.0), c[3]]
    });

    let out_norm = Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = norm.get(x, y, tile);
        let nx = (n[0] * 2.0 - 1.0) * n_factor;
        let ny = (n[1] * 2.0 - 1.0) * n_factor;
        let nz = (n[2] * 2.0 - 1.0).max(0.01);
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len * 0.5 + 0.5, ny / len * 0.5 + 0.5, nz / len * 0.5 + 0.5, 1.0]
    });

    let out_rough = Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        (rough.get_gray(x, y, tile) + r_bias).clamp(0.0, 1.0)
    });

    Ok(vec![out_bc, out_norm, out_rough])
}

fn cook_material_switch(context: &CookContext<'_>) -> CookResult {
    let sel = context.b("select_mat2");
    if sel {
        Ok(vec![
            context.any_or(3, Kind::Color, [0.0, 0.0, 0.0, 1.0]),
            context.any_or(4, Kind::Color, [0.5, 0.5, 1.0, 1.0]),
            context.any_or(5, Kind::Gray, [0.5; 4]),
        ])
    } else {
        Ok(vec![
            context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]),
            context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]),
            context.any_or(2, Kind::Gray, [0.5; 4]),
        ])
    }
}

fn cook_multi_material_blend(context: &CookContext<'_>) -> CookResult {
    let base = context.any_or(0, Kind::Color, [0.1, 0.1, 0.1, 1.0]);
    let l1 = context.any_or(1, Kind::Color, [0.8, 0.2, 0.2, 1.0]);
    let m1 = context.sized(2).map(|m| m.to_gray());
    let l2 = context.any_or(3, Kind::Color, [0.2, 0.8, 0.2, 1.0]);
    let m2 = context.sized(4).map(|m| m.to_gray());
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let b = base.get(x, y, tile);
        let c1 = l1.get(x, y, tile);
        let w1 = m1.as_ref().map_or(1.0, |m| m.get_gray(x, y, tile));

        let r1 = [b[0] * (1.0 - w1) + c1[0] * w1, b[1] * (1.0 - w1) + c1[1] * w1, b[2] * (1.0 - w1) + c1[2] * w1];

        let c2 = l2.get(x, y, tile);
        let w2 = m2.as_ref().map_or(0.0, |m| m.get_gray(x, y, tile));

        [
            (r1[0] * (1.0 - w2) + c2[0] * w2).clamp(0.0, 1.0),
            (r1[1] * (1.0 - w2) + c2[1] * w2).clamp(0.0, 1.0),
            (r1[2] * (1.0 - w2) + c2[2] * w2).clamp(0.0, 1.0),
            1.0,
        ]
    })])
}
