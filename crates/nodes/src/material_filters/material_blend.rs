use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("material_blend", "Material Blend", CATEGORY)
        .desc("Blends two complete PBR material channel sets using a mask.")
        .color_in("base_color_top", "Base Color Top")
        .primary_input("base_color_top")
        .color_in("base_color_bottom", "Base Color Bottom")
        .color_in("normal_top", "Normal Top")
        .color_in("normal_bottom", "Normal Bottom")
        .gray_in("roughness_top", "Roughness Top")
        .gray_in("roughness_bottom", "Roughness Bottom")
        .gray_in("metallic_top", "Metallic Top")
        .gray_in("metallic_bottom", "Metallic Bottom")
        .gray_in("height_top", "Height Top")
        .gray_in("height_bottom", "Height Bottom")
        .gray_in("mask", "Mask")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("roughness", "Roughness")
        .named_gray_out("metallic", "Metallic")
        .named_gray_out("height", "Height")
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let opacity = context.f("opacity").clamp(0.0, 1.0);
    let mask = context.gray_or(10, 1.0);

    let bc_top = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let bc_bottom = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);

    let norm_top = context.any_or(2, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let norm_bottom = context.any_or(3, Kind::Color, [0.5, 0.5, 1.0, 1.0]);

    let rough_top = context.gray_or(4, 0.5);
    let rough_bottom = context.gray_or(5, 0.5);

    let metal_top = context.gray_or(6, 0.0);
    let metal_bottom = context.gray_or(7, 0.0);

    let height_top = context.gray_or(8, 0.5);
    let height_bottom = context.gray_or(9, 0.0);

    let num_pixels = (context.w * context.h) as usize;

    let mut bc_data = vec![0.0f32; num_pixels * 4];
    let mut norm_data = vec![0.0f32; num_pixels * 4];
    let mut rough_data = vec![0.0f32; num_pixels];
    let mut metal_data = vec![0.0f32; num_pixels];
    let mut height_data = vec![0.0f32; num_pixels];

    let m_data = mask.data();
    let bt_data = bc_top.data();
    let bb_data = bc_bottom.data();
    let nt_data = norm_top.data();
    let nb_data = norm_bottom.data();
    let rt_data = rough_top.data();
    let rb_data = rough_bottom.data();
    let mt_data = metal_top.data();
    let mb_data = metal_bottom.data();
    let ht_data = height_top.data();
    let hb_data = height_bottom.data();

    for i in 0..num_pixels {
        let t = (m_data.get(i).copied().unwrap_or(1.0) * opacity).clamp(0.0, 1.0);
        let inv_t = 1.0 - t;

        // Base color (RGBA)
        let c_idx = i * 4;
        for c in 0..4 {
            let vt = bt_data.get(c_idx + c).copied().unwrap_or(0.0);
            let vb = bb_data.get(c_idx + c).copied().unwrap_or(0.0);
            if let Some(target) = bc_data.get_mut(c_idx + c) {
                *target = (vb * inv_t + vt * t).clamp(0.0, 1.0);
            }
        }

        // Normal (RGB) normalized blend
        let nt_x = nt_data.get(c_idx).copied().unwrap_or(0.5) * 2.0 - 1.0;
        let nt_y = nt_data.get(c_idx + 1).copied().unwrap_or(0.5) * 2.0 - 1.0;
        let nt_z = nt_data.get(c_idx + 2).copied().unwrap_or(1.0) * 2.0 - 1.0;

        let nb_x = nb_data.get(c_idx).copied().unwrap_or(0.5) * 2.0 - 1.0;
        let nb_y = nb_data.get(c_idx + 1).copied().unwrap_or(0.5) * 2.0 - 1.0;
        let nb_z = nb_data.get(c_idx + 2).copied().unwrap_or(1.0) * 2.0 - 1.0;

        let nx = nb_x * inv_t + nt_x * t;
        let ny = nb_y * inv_t + nt_y * t;
        let nz = nb_z * inv_t + nt_z * t;
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);

        if let Some(target) = norm_data.get_mut(c_idx) {
            *target = ((nx / len) * 0.5 + 0.5).clamp(0.0, 1.0);
        }
        if let Some(target) = norm_data.get_mut(c_idx + 1) {
            *target = ((ny / len) * 0.5 + 0.5).clamp(0.0, 1.0);
        }
        if let Some(target) = norm_data.get_mut(c_idx + 2) {
            *target = ((nz / len) * 0.5 + 0.5).clamp(0.0, 1.0);
        }
        if let Some(target) = norm_data.get_mut(c_idx + 3) {
            *target = 1.0;
        }

        // Roughness
        let rt = rt_data.get(i).copied().unwrap_or(0.5);
        let rb = rb_data.get(i).copied().unwrap_or(0.5);
        if let Some(target) = rough_data.get_mut(i) {
            *target = (rb * inv_t + rt * t).clamp(0.0, 1.0);
        }

        // Metallic
        let mt = mt_data.get(i).copied().unwrap_or(0.0);
        let mb = mb_data.get(i).copied().unwrap_or(0.0);
        if let Some(target) = metal_data.get_mut(i) {
            *target = (mb * inv_t + mt * t).clamp(0.0, 1.0);
        }

        // Height
        let ht = ht_data.get(i).copied().unwrap_or(0.5);
        let hb = hb_data.get(i).copied().unwrap_or(0.0);
        if let Some(target) = height_data.get_mut(i) {
            *target = (hb * inv_t + ht * t).clamp(0.0, 1.0);
        }
    }

    let limits = craft_texture::Limits::default();
    let out_bc = Image::from_data(context.w, context.h, Kind::Color, bc_data, &limits).map_err(|e| e.to_string())?;
    let out_norm = Image::from_data(context.w, context.h, Kind::Color, norm_data, &limits).map_err(|e| e.to_string())?;
    let out_rough = Image::from_data(context.w, context.h, Kind::Gray, rough_data, &limits).map_err(|e| e.to_string())?;
    let out_metal = Image::from_data(context.w, context.h, Kind::Gray, metal_data, &limits).map_err(|e| e.to_string())?;
    let out_height = Image::from_data(context.w, context.h, Kind::Gray, height_data, &limits).map_err(|e| e.to_string())?;

    Ok(vec![out_bc, out_norm, out_rough, out_metal, out_height])
}
