use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition_bitmap_to_material_light() -> NodeDefinition {
    node("bitmap_to_material_light", "Bitmap to Material Light", CATEGORY)
        .desc("Extracts Albedo, Normal, Roughness and Height maps from a single photograph or diffuse bitmap.")
        .color_in("bitmap", "Bitmap Image")
        .primary_input("bitmap")
        .float("normal_intensity", "Normal Intensity", 1.0, 0.0, 5.0)
        .float("roughness_scale", "Roughness Scale", 1.0, 0.0, 2.0)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("roughness", "Roughness")
        .named_gray_out("height", "Height")
        .cook(cook_bitmap_to_material_light)
}

pub(crate) fn definition_season_filter() -> NodeDefinition {
    node("season_filter", "Season Filter", CATEGORY)
        .desc("Modulates vegetation and environmental material color palettes according to seasonal cycles.")
        .color_in("albedo", "Albedo")
        .primary_input("albedo")
        .choice("season", "Season", &["Spring", "Summer", "Autumn", "Winter"], 2)
        .float("intensity", "Intensity", 1.0, 0.0, 1.0)
        .color_out()
        .cook(cook_season_filter)
}

pub(crate) fn definition_snow_cover() -> NodeDefinition {
    node("snow_cover", "Snow Cover", CATEGORY)
        .desc("Accumulates realistic snow layering on upward-facing surface regions and low crevices.")
        .color_in("albedo", "Albedo")
        .primary_input("albedo")
        .color_in("normal", "Normal")
        .gray_in("height", "Height")
        .float("snow_amount", "Snow Amount", 0.5, 0.0, 1.0)
        .float("crevice_fill", "Crevice Fill", 0.3, 0.0, 1.0)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("snow_mask", "Snow Mask")
        .cook(cook_snow_cover)
}

pub(crate) fn definition_water_level() -> NodeDefinition {
    node("water_level", "Water Level", CATEGORY)
        .desc("Submerges material lower elevations beneath a planar water table with surface ripples.")
        .color_in("albedo", "Albedo")
        .primary_input("albedo")
        .color_in("normal", "Normal")
        .gray_in("height", "Height")
        .float("water_level", "Water Level", 0.3, 0.0, 1.0)
        .float("clarity", "Water Clarity", 0.7, 0.0, 1.0)
        .named_color_out("base_color", "Base Color")
        .named_color_out("normal", "Normal")
        .named_gray_out("water_mask", "Water Mask")
        .cook(cook_water_level)
}

pub(crate) fn definition_material_transform() -> NodeDefinition {
    node("material_transform", "Material Transform", CATEGORY)
        .desc("Synchronously tiles, scales and rotates all channels of a PBR material set.")
        .color_in("albedo", "Albedo")
        .primary_input("albedo")
        .color_in("normal", "Normal")
        .gray_in("roughness", "Roughness")
        .float2("offset", "Offset", [0.0, 0.0], -1.0, 1.0)
        .float2("tiling", "Tiling", [1.0, 1.0], 0.1, 16.0)
        .angle("rotation", "Rotation", 0.0)
        .named_color_out("albedo", "Albedo")
        .named_color_out("normal", "Normal")
        .named_gray_out("roughness", "Roughness")
        .cook(cook_material_transform)
}

fn cook_bitmap_to_material_light(context: &CookContext<'_>) -> CookResult {
    let bmp = context.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]);
    let (w, h) = (context.w, context.h);
    let n_intensity = context.f("normal_intensity").max(0.0);
    let r_scale = context.f("roughness_scale").max(0.0);
    let tile = context.tile;

    let base_color = bmp.clone();

    let height = Image::gray(w, h, |x, y| {
        let c = bmp.get(x as i64, y as i64, tile);
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    });

    let normal = Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let r = height.get_gray(x + 1, y, tile);
        let l = height.get_gray(x - 1, y, tile);
        let d = height.get_gray(x, y + 1, tile);
        let u = height.get_gray(x, y - 1, tile);

        let dx = (r - l) * n_intensity * 2.0;
        let dy = (d - u) * n_intensity * 2.0;
        let len = (dx * dx + dy * dy + 1.0).sqrt();
        [-dx / len * 0.5 + 0.5, dy / len * 0.5 + 0.5, 1.0 / len * 0.5 + 0.5, 1.0]
    });

    let roughness = Image::gray(w, h, |x, y| {
        let h_val = height.get_gray(x as i64, y as i64, tile);
        ((1.0 - h_val) * r_scale).clamp(0.05, 0.95)
    });

    Ok(vec![base_color, normal, roughness, height])
}

fn cook_season_filter(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [0.2, 0.5, 0.2, 1.0]);
    let season = context.i("season");
    let intensity = context.f("intensity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    // Palette tint per season
    let tint = match season {
        0 => [1.1, 1.2, 0.9],  // Spring: vibrant yellow-green
        1 => [0.9, 1.1, 0.9],  // Summer: deep lush green
        2 => [1.3, 0.7, 0.2],  // Autumn: orange-amber
        _ => [0.8, 0.85, 1.0], // Winter: desaturated cold blue
    };

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let c = albedo.get(x as i64, y as i64, tile);
        let target = [(c[0] * tint[0]).clamp(0.0, 1.0), (c[1] * tint[1]).clamp(0.0, 1.0), (c[2] * tint[2]).clamp(0.0, 1.0)];
        [
            c[0] * (1.0 - intensity) + target[0] * intensity,
            c[1] * (1.0 - intensity) + target[1] * intensity,
            c[2] * (1.0 - intensity) + target[2] * intensity,
            c[3],
        ]
    })])
}

fn cook_snow_cover(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [0.3, 0.2, 0.1, 1.0]);
    let normal = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let height = context.any_or(2, Kind::Gray, [0.5; 4]);
    let snow_amt = context.f("snow_amount").clamp(0.0, 1.0);
    let crevice = context.f("crevice_fill").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let snow_color = [0.95, 0.97, 1.0, 1.0];

    let mut out_bc = vec![[0.0f32; 4]; (w * h) as usize];
    let mut out_norm = vec![[0.5f32, 0.5, 1.0, 1.0]; (w * h) as usize];
    let mut out_mask = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;
            let n = normal.get(xi, yi, tile);
            let nz = (n[2] * 2.0 - 1.0).max(0.0);
            let h_val = height.get_gray(xi, yi, tile);

            // Upward surfaces get snow first; crevice filling accumulates snow in cavities
            let upward = nz.powf(2.0);
            let cavity = (1.0 - h_val) * crevice;
            let coverage = ((upward * snow_amt * 1.5 + cavity) - (1.0 - snow_amt) * 0.5).clamp(0.0, 1.0);

            let c = albedo.get(xi, yi, tile);
            out_bc[idx] = [
                c[0] * (1.0 - coverage) + snow_color[0] * coverage,
                c[1] * (1.0 - coverage) + snow_color[1] * coverage,
                c[2] * (1.0 - coverage) + snow_color[2] * coverage,
                1.0,
            ];

            let sn_norm = [0.5, 0.5, 1.0];
            out_norm[idx] = [
                n[0] * (1.0 - coverage) + sn_norm[0] * coverage,
                n[1] * (1.0 - coverage) + sn_norm[1] * coverage,
                n[2] * (1.0 - coverage) + sn_norm[2] * coverage,
                1.0,
            ];
            out_mask[idx] = coverage;
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(w, h, Kind::Color, out_bc.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Color, out_norm.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_mask, &limits).map_err(|e| e.to_string())?,
    ])
}

fn cook_water_level(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [0.3, 0.2, 0.1, 1.0]);
    let normal = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let height = context.any_or(2, Kind::Gray, [0.5; 4]);
    let level = context.f("water_level").clamp(0.0, 1.0);
    let clarity = context.f("clarity").clamp(0.01, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let water_tint = [0.05, 0.2, 0.35, 1.0];

    let mut out_bc = vec![[0.0f32; 4]; (w * h) as usize];
    let mut out_norm = vec![[0.5f32, 0.5, 1.0, 1.0]; (w * h) as usize];
    let mut out_mask = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;
            let h_val = height.get_gray(xi, yi, tile);

            let under = if h_val < level { ((level - h_val) / level.max(0.01)).clamp(0.0, 1.0) } else { 0.0 };
            let depth_fog = (under * (1.0 - clarity)).clamp(0.0, 0.9);

            let c = albedo.get(xi, yi, tile);
            let n = normal.get(xi, yi, tile);

            if under > 0.0 {
                out_bc[idx] = [
                    c[0] * (1.0 - depth_fog) + water_tint[0] * depth_fog,
                    c[1] * (1.0 - depth_fog) + water_tint[1] * depth_fog,
                    c[2] * (1.0 - depth_fog) + water_tint[2] * depth_fog,
                    1.0,
                ];
                // Water surface flattens the normal
                out_norm[idx] = [n[0] * (1.0 - under) + 0.5 * under, n[1] * (1.0 - under) + 0.5 * under, n[2] * (1.0 - under) + 1.0 * under, 1.0];
                out_mask[idx] = 1.0;
            } else {
                out_bc[idx] = c;
                out_norm[idx] = n;
                out_mask[idx] = 0.0;
            }
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(w, h, Kind::Color, out_bc.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Color, out_norm.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_mask, &limits).map_err(|e| e.to_string())?,
    ])
}

fn cook_material_transform(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let normal = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let rough = context.any_or(2, Kind::Gray, [0.5; 4]);
    let offset = context.v2("offset");
    let tiling = context.v2("tiling");
    let rot = context.f("rotation") * std::f32::consts::PI * 2.0;
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let cos_r = rot.cos();
    let sin_r = rot.sin();
    let tx = tiling[0].max(0.01);
    let ty = tiling[1].max(0.01);

    let out_albedo = Image::generate_uv(w, h, Kind::Color, |u, v| {
        let x = (u - 0.5) * tx;
        let y = (v - 0.5) * ty;
        let su = cos_r * x - sin_r * y + 0.5 + offset[0];
        let sv = sin_r * x + cos_r * y + 0.5 + offset[1];
        albedo.sample_axes(su, sv, [tile, tile])
    });

    let out_normal = Image::generate_uv(w, h, Kind::Color, |u, v| {
        let x = (u - 0.5) * tx;
        let y = (v - 0.5) * ty;
        let su = cos_r * x - sin_r * y + 0.5 + offset[0];
        let sv = sin_r * x + cos_r * y + 0.5 + offset[1];
        let n = normal.sample_axes(su, sv, [tile, tile]);
        let nx = n[0] * 2.0 - 1.0;
        let ny = n[1] * 2.0 - 1.0;
        let r_nx = cos_r * nx - sin_r * ny;
        let r_ny = sin_r * nx + cos_r * ny;
        [r_nx * 0.5 + 0.5, r_ny * 0.5 + 0.5, n[2], n[3]]
    });

    let out_rough = Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = (u - 0.5) * tx;
        let y = (v - 0.5) * ty;
        let su = cos_r * x - sin_r * y + 0.5 + offset[0];
        let sv = sin_r * x + cos_r * y + 0.5 + offset[1];
        rough.sample_axes(su, sv, [tile, tile])
    });

    Ok(vec![out_albedo, out_normal, out_rough])
}
