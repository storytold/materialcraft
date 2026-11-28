use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub const CATEGORY: &str = "Filters/Effects";

pub(crate) fn definition_edge_wear() -> NodeDefinition {
    node("edge_wear", "Edge Wear", CATEGORY)
        .desc("Simulates chipping, flaking and abrasion along high-curvature surface edges.")
        .gray_in("curvature", "Curvature / Edges")
        .primary_input("curvature")
        .gray_in("noise", "Noise / Grunge")
        .float("wear_amount", "Wear Amount", 0.5, 0.0, 1.0)
        .float("contrast", "Edge Contrast", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_edge_wear)
}

pub(crate) fn definition_metal_edge_wear() -> NodeDefinition {
    node("metal_edge_wear", "Metal Edge Wear", CATEGORY)
        .desc("Specialized edge wear revealing bright underlying bare metal along exposed geometry borders.")
        .color_in("albedo", "Painted Albedo")
        .primary_input("albedo")
        .gray_in("curvature", "Curvature Map")
        .gray_in("grunge", "Grunge / Scratches")
        .float("wear_level", "Wear Level", 0.4, 0.0, 1.0)
        .named_color_out("base_color", "Worn Albedo")
        .named_gray_out("wear_mask", "Wear Mask")
        .cook(cook_metal_edge_wear)
}

pub(crate) fn definition_weathering_dirt() -> NodeDefinition {
    node("weathering_dirt", "Weathering Dirt", CATEGORY)
        .desc("Accumulates grime and dirt into ambient occlusion crevices and low-slope flats.")
        .gray_in("ambient_occlusion", "Ambient Occlusion")
        .primary_input("ambient_occlusion")
        .gray_in("grunge", "Dirt Texture")
        .float("dirt_amount", "Dirt Amount", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_weathering_dirt)
}

pub(crate) fn definition_dripping_rust() -> NodeDefinition {
    node("dripping_rust", "Dripping Rust", CATEGORY)
        .desc("Simulates vertical gravitational rust and fluid streaks running down metallic surfaces.")
        .gray_in("input", "Source Edges / Crevices")
        .primary_input("input")
        .float("drip_length", "Drip Length", 0.4, 0.01, 1.0)
        .float("intensity", "Rust Intensity", 0.7, 0.0, 1.0)
        .gray_out()
        .cook(cook_dripping_rust)
}

pub(crate) fn definition_dust() -> NodeDefinition {
    node("dust", "Dust", CATEGORY)
        .desc("Settles realistic fine particle dust layers onto upward-facing surface regions.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .gray_in("grunge", "Dust Grain")
        .float("dust_amount", "Dust Amount", 0.3, 0.0, 1.0)
        .gray_out()
        .cook(cook_dust)
}

pub(crate) fn definition_edge_blur() -> NodeDefinition {
    node("edge_blur", "Edge Blur", CATEGORY)
        .desc("Softens and anti-aliases binary mask boundaries while preserving interior sharpness.")
        .gray_in("input", "Mask Input")
        .primary_input("input")
        .float("radius", "Blur Radius", 2.0, 0.1, 16.0)
        .gray_out()
        .cook(cook_edge_blur)
}

pub(crate) fn definition_edge_damages() -> NodeDefinition {
    node("edge_damages", "Edge Damages", CATEGORY)
        .desc("Generates jagged gouges, chips and impacts along high-contrast mask contours.")
        .gray_in("input", "Edge / Silhouette")
        .primary_input("input")
        .gray_in("noise", "Damage Noise")
        .float("damage_scale", "Damage Depth", 0.3, 0.0, 1.0)
        .gray_out()
        .cook(cook_edge_damages)
}

pub(crate) fn definition_edge_dirt() -> NodeDefinition {
    node("edge_dirt", "Edge Dirt", CATEGORY)
        .desc("Accumulates oily stains and dirt residues along borders and corner creases.")
        .gray_in("curvature", "Curvature Map")
        .primary_input("curvature")
        .gray_in("grunge", "Grunge Map")
        .float("amount", "Dirt Amount", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_edge_dirt)
}

pub(crate) fn definition_grease() -> NodeDefinition {
    node("grease", "Grease", CATEGORY)
        .desc("Generates organic oily stains, smudges and fingerprint residue masks.")
        .gray_in("grunge", "Grunge Input")
        .primary_input("grunge")
        .float("spread", "Grease Spread", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_grease)
}

pub(crate) fn definition_ground_dirt() -> NodeDefinition {
    node("ground_dirt", "Ground Dirt", CATEGORY)
        .desc("Simulates upward dirt and mud splash gradient rising from the ground plane.")
        .float("height", "Gradient Height", 0.4, 0.01, 1.0)
        .float("contrast", "Gradient Contrast", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_ground_dirt)
}

pub(crate) fn definition_leaks() -> NodeDefinition {
    node("leaks", "Leaks", CATEGORY)
        .desc("Generates vertical dripping streaks and mineral deposits flowing from top down.")
        .gray_in("noise", "Seed Noise")
        .primary_input("noise")
        .float("length", "Leak Length", 0.5, 0.05, 1.0)
        .float("width", "Leak Width", 0.2, 0.01, 1.0)
        .gray_out()
        .cook(cook_leaks)
}

pub(crate) fn definition_light() -> NodeDefinition {
    node("light", "Light", CATEGORY)
        .desc("Applies directional or point illumination to surface normals and height maps.")
        .color_in("normal", "Normal Map")
        .primary_input("normal")
        .angle("light_angle", "Light Direction", 0.125)
        .float("elevation", "Light Elevation", 0.5, 0.0, 1.0)
        .color("light_color", "Light Color", [1.0, 0.95, 0.9, 1.0])
        .color_out()
        .cook(cook_light)
}

pub(crate) fn definition_shadows() -> NodeDefinition {
    node("shadows", "Shadows", CATEGORY)
        .desc("Computes raymarched directional cast shadows across a height field.")
        .gray_in("height", "Height Map")
        .primary_input("height")
        .angle("sun_angle", "Sun Azimuth", 0.125)
        .float("sun_elevation", "Sun Elevation", 0.3, 0.05, 1.0)
        .float("shadow_distance", "Ray Distance", 0.3, 0.01, 1.0)
        .gray_out()
        .cook(cook_shadows)
}

pub(crate) fn definition_sun_bleach() -> NodeDefinition {
    node("sun_bleach", "Sun Bleach", CATEGORY)
        .desc("Fades and desaturates upward-facing surface colors exposed to solar UV radiation.")
        .color_in("albedo", "Albedo")
        .primary_input("albedo")
        .color_in("normal", "Normal Map")
        .float("bleach_intensity", "Bleach Intensity", 0.5, 0.0, 1.0)
        .color_out()
        .cook(cook_sun_bleach)
}

pub(crate) fn definition_cracks_weathering() -> NodeDefinition {
    node("cracks_weathering", "Cracks Weathering", CATEGORY)
        .desc("Generates aging cracks, fissures and fractured crevice networks.")
        .float("scale", "Crack Scale", 8.0, 1.0, 32.0)
        .float("width", "Crack Width", 0.05, 0.005, 0.3)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_cracks_weathering)
}

pub(crate) fn definition_rock_weathering() -> NodeDefinition {
    node("rock_weathering", "Rock Weathering", CATEGORY)
        .desc("Simulates geologic strata weathering, crumbling edges and mineral erosion.")
        .gray_in("rock_height", "Rock Height")
        .primary_input("rock_height")
        .float("erosion", "Erosion Depth", 0.4, 0.0, 1.0)
        .gray_out()
        .cook(cook_rock_weathering)
}

pub(crate) fn definition_rust_weathering() -> NodeDefinition {
    node("rust_weathering", "Rust Weathering", CATEGORY)
        .desc("Multi-layered oxidation mask simulating blistering and granular surface corrosion.")
        .gray_in("grunge", "Grunge Map")
        .primary_input("grunge")
        .float("rust_level", "Rust Level", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_rust_weathering)
}

fn cook_edge_wear(context: &CookContext<'_>) -> CookResult {
    let curv = context.any_or(0, Kind::Gray, [0.5; 4]);
    let noise = context.sized(1).map(|m| m.to_gray());
    let wear = context.f("wear_amount").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;
    let slope = 1.0 / (1.0 - contrast * 0.95);

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let c = curv.get_gray(x, y, tile);
        let n = noise.as_ref().map_or(0.5, |img| img.get_gray(x, y, tile));
        let val = (c * 1.5 + n * 0.5) * wear;
        ((val - 0.5) * slope + 0.5).clamp(0.0, 1.0)
    })])
}

fn cook_metal_edge_wear(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [0.2, 0.3, 0.6, 1.0]);
    let curv = context.any_or(1, Kind::Gray, [0.5; 4]);
    let grunge = context.sized(2).map(|m| m.to_gray());
    let wear = context.f("wear_level").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let metal_albedo = [0.85, 0.85, 0.88, 1.0];

    let mut out_bc = vec![[0.0f32; 4]; (w * h) as usize];
    let mut out_mask = vec![0.0f32; (w * h) as usize];

    for y in 0..h {
        let yi = y as i64;
        for x in 0..w {
            let xi = x as i64;
            let idx = (y * w + x) as usize;
            let c = curv.get_gray(xi, yi, tile);
            let g = grunge.as_ref().map_or(0.5, |m| m.get_gray(xi, yi, tile));

            let m = ((c * 1.8 + g * 0.4) * wear - (1.0 - wear) * 0.5).clamp(0.0, 1.0);
            let alb = albedo.get(xi, yi, tile);

            out_bc[idx] =
                [alb[0] * (1.0 - m) + metal_albedo[0] * m, alb[1] * (1.0 - m) + metal_albedo[1] * m, alb[2] * (1.0 - m) + metal_albedo[2] * m, alb[3]];
            out_mask[idx] = m;
        }
    }

    let limits = craft_texture::Limits::default();
    Ok(vec![
        Image::from_data(w, h, Kind::Color, out_bc.into_iter().flatten().collect::<Vec<f32>>(), &limits).map_err(|e| e.to_string())?,
        Image::from_data(w, h, Kind::Gray, out_mask, &limits).map_err(|e| e.to_string())?,
    ])
}

fn cook_weathering_dirt(context: &CookContext<'_>) -> CookResult {
    let ao = context.any_or(0, Kind::Gray, [1.0; 4]);
    let grunge = context.sized(1).map(|m| m.to_gray());
    let amt = context.f("dirt_amount").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let a = ao.get_gray(x, y, tile);
        let g = grunge.as_ref().map_or(0.5, |m| m.get_gray(x, y, tile));
        let crevice = (1.0 - a).max(0.0);
        (crevice * 1.5 * g * amt).clamp(0.0, 1.0)
    })])
}

fn cook_dripping_rust(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let drip_len = context.f("drip_length").clamp(0.01, 1.0);
    let intensity = context.f("intensity").clamp(0.0, 1.0);
    let tile = context.tile;
    let drip_px = (drip_len * h as f32).round() as i64;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let mut best = input.get_gray(x, y, tile);
        for dy in 1..=drip_px {
            let prev = input.get_gray(x, y - dy, tile);
            let falloff = 1.0 - (dy as f32 / drip_px as f32);
            best = best.max(prev * falloff * intensity);
        }
        best.clamp(0.0, 1.0)
    })])
}

fn cook_dust(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let grunge = context.sized(1).map(|m| m.to_gray());
    let amt = context.f("dust_amount").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = normal.get(x, y, tile);
        let up = (n[2] * 2.0 - 1.0).max(0.0);
        let g = grunge.as_ref().map_or(0.5, |m| m.get_gray(x, y, tile));
        (up.powf(1.5) * g * amt * 2.0).clamp(0.0, 1.0)
    })])
}

fn cook_edge_blur(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let radius = context.f("radius").clamp(0.1, 32.0).round() as i64;
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let mut sum = 0.0f32;
        let mut count = 0.0f32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    sum += input.get_gray(x + dx, y + dy, tile);
                    count += 1.0;
                }
            }
        }
        sum / count.max(1.0)
    })])
}

fn cook_edge_damages(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let noise = context.sized(1).map(|m| m.to_gray());
    let scale = context.f("damage_scale").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let base = input.get_gray(x, y, tile);
        let n = noise.as_ref().map_or(0.5, |m| m.get_gray(x, y, tile));
        (base - (1.0 - n) * scale).clamp(0.0, 1.0)
    })])
}

fn cook_edge_dirt(context: &CookContext<'_>) -> CookResult {
    let curv = context.any_or(0, Kind::Gray, [0.5; 4]);
    let grunge = context.sized(1).map(|m| m.to_gray());
    let amt = context.f("amount").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let c = curv.get_gray(x, y, tile);
        let g = grunge.as_ref().map_or(0.5, |m| m.get_gray(x, y, tile));
        let edge = (c - 0.5).abs() * 2.0;
        (edge * g * amt * 2.0).clamp(0.0, 1.0)
    })])
}

fn cook_grease(context: &CookContext<'_>) -> CookResult {
    let grunge = context.any_or(0, Kind::Gray, [0.5; 4]);
    let spread = context.f("spread").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let g = grunge.get_gray(x, y, tile);
        ((g - (1.0 - spread) * 0.5) / spread.max(0.01)).clamp(0.0, 1.0)
    })])
}

fn cook_ground_dirt(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let height = context.f("height").clamp(0.01, 1.0);
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let slope = 1.0 / (1.0 - contrast * 0.95);

    Ok(vec![Image::gray(w, h, |_x, y| {
        let v = 1.0 - (y as f32 + 0.5) / h as f32; // 0 at bottom, 1 at top
        let d = ((height - v) / height).clamp(0.0, 1.0);
        ((d - 0.5) * slope + 0.5).clamp(0.0, 1.0)
    })])
}

fn cook_leaks(context: &CookContext<'_>) -> CookResult {
    let noise = context.any_or(0, Kind::Gray, [0.5; 4]);
    let (w, h) = (context.w, context.h);
    let len = context.f("length").clamp(0.05, 1.0);
    let width = context.f("width").clamp(0.01, 1.0);
    let tile = context.tile;
    let len_px = (len * h as f32).round() as i64;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let mut accum = 0.0f32;
        for dy in 0..=len_px {
            let n = noise.get_gray(x, y - dy, tile);
            if n > 1.0 - width {
                accum = accum.max(1.0 - (dy as f32 / len_px as f32));
            }
        }
        accum.clamp(0.0, 1.0)
    })])
}

fn cook_light(context: &CookContext<'_>) -> CookResult {
    let normal = context.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let (w, h) = (context.w, context.h);
    let angle = context.f("light_angle") * std::f32::consts::PI * 2.0;
    let elev = context.f("elevation").clamp(0.0, 1.0);
    let l_col = context.v4("light_color");
    let tile = context.tile;

    let lx = angle.cos() * (1.0 - elev);
    let ly = angle.sin() * (1.0 - elev);
    let lz = elev;
    let l_len = (lx * lx + ly * ly + lz * lz).sqrt();
    let (lx, ly, lz) = (lx / l_len, ly / l_len, lz / l_len);

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let n = normal.get(x, y, tile);
        let nx = n[0] * 2.0 - 1.0;
        let ny = n[1] * 2.0 - 1.0;
        let nz = (n[2] * 2.0 - 1.0).max(0.01);

        let diff = (nx * lx + ny * ly + nz * lz).max(0.0);
        [diff * l_col[0], diff * l_col[1], diff * l_col[2], 1.0]
    })])
}

fn cook_shadows(context: &CookContext<'_>) -> CookResult {
    let height = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let sun_ang = context.f("sun_angle") * std::f32::consts::PI * 2.0;
    let sun_elev = context.f("sun_elevation").clamp(0.01, 1.0);
    let dist = context.f("shadow_distance").clamp(0.01, 1.0);
    let tile = context.tile;

    let steps = 16;
    let max_dist_px = dist * (w.min(h) as f32);
    let ray_dx = sun_ang.cos();
    let ray_dy = sun_ang.sin();
    let tan_elev = sun_elev.max(0.05);

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let h_origin = height.get_gray(x, y, tile);
        let mut shadow = 1.0f32;

        for s in 1..=steps {
            let step_dist = (s as f32 / steps as f32) * max_dist_px;
            let sx = x + (ray_dx * step_dist).round() as i64;
            let sy = y + (ray_dy * step_dist).round() as i64;
            let h_sample = height.get_gray(sx, sy, tile);
            let ray_h = h_origin + step_dist * tan_elev * (1.0 / (w as f32));
            if h_sample > ray_h {
                shadow = 0.0;
                break;
            }
        }
        shadow
    })])
}

fn cook_sun_bleach(context: &CookContext<'_>) -> CookResult {
    let albedo = context.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]);
    let normal = context.any_or(1, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let intensity = context.f("bleach_intensity").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let c = albedo.get(x, y, tile);
        let n = normal.get(x, y, tile);
        let upward = (n[2] * 2.0 - 1.0).max(0.0);
        let lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];

        let bleach = (upward * intensity).clamp(0.0, 1.0);
        [
            (c[0] * (1.0 - bleach) + (c[0] * 0.5 + lum * 0.5) * bleach).clamp(0.0, 1.0),
            (c[1] * (1.0 - bleach) + (c[1] * 0.5 + lum * 0.5) * bleach).clamp(0.0, 1.0),
            (c[2] * (1.0 - bleach) + (c[2] * 0.5 + lum * 0.5) * bleach).clamp(0.0, 1.0),
            c[3],
        ]
    })])
}

fn cook_cracks_weathering(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let width = context.f("width").clamp(0.001, 0.5);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::gray(w, h, |x, y| {
        let u = (x as f32 + 0.5) / w as f32 * scale;
        let v = (y as f32 + 0.5) / h as f32 * scale;
        let xi = u.floor() as i32;
        let yi = v.floor() as i32;
        let h_hash = ((xi as u32).wrapping_mul(0x1f1f1f1f) ^ (yi as u32).wrapping_mul(0x85ebca6b) ^ seed) as f32 / 4_294_967_295.0;
        let fx = (u - xi as f32 - 0.5).abs();
        let fy = (v - yi as f32 - 0.5).abs();
        let edge_dist = fx.min(fy);
        if edge_dist < width * (0.5 + 0.5 * h_hash) { 1.0 - edge_dist / (width * (0.5 + 0.5 * h_hash)) } else { 0.0 }
    })])
}

fn cook_rock_weathering(context: &CookContext<'_>) -> CookResult {
    let height = context.any_or(0, Kind::Gray, [0.5; 4]);
    let erosion = context.f("erosion").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let h_val = height.get_gray(x, y, tile);
        let r = height.get_gray(x + 1, y, tile);
        let l = height.get_gray(x - 1, y, tile);
        let d = height.get_gray(x, y + 1, tile);
        let u = height.get_gray(x, y - 1, tile);
        let slope = ((r - l).powi(2) + (d - u).powi(2)).sqrt();
        (h_val - slope * erosion).clamp(0.0, 1.0)
    })])
}

fn cook_rust_weathering(context: &CookContext<'_>) -> CookResult {
    let grunge = context.any_or(0, Kind::Gray, [0.5; 4]);
    let rust = context.f("rust_level").clamp(0.0, 1.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    Ok(vec![Image::gray(w, h, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let g = grunge.get_gray(x, y, tile);
        ((g - (1.0 - rust) * 0.7) / 0.3).clamp(0.0, 1.0)
    })])
}

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        definition_edge_wear(),
        definition_metal_edge_wear(),
        definition_weathering_dirt(),
        definition_dripping_rust(),
        definition_dust(),
        definition_edge_blur(),
        definition_edge_damages(),
        definition_edge_dirt(),
        definition_grease(),
        definition_ground_dirt(),
        definition_leaks(),
        definition_light(),
        definition_shadows(),
        definition_sun_bleach(),
        definition_cracks_weathering(),
        definition_rock_weathering(),
        definition_rust_weathering(),
    ]
}
