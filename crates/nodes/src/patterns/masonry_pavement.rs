use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn alveolus_definition() -> NodeDefinition {
    node("alveolus", "Alveolus", CATEGORY)
        .desc("Hexagonal porous cell honeycomb with beveled spherical cavity walls.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("wall_thickness", "Wall Thickness", 0.15, 0.01, 0.45)
        .float("depth", "Cavity Depth", 0.8, 0.1, 1.0)
        .gray_out()
        .cook(cook_alveolus)
}

pub(crate) fn arc_pavement_definition() -> NodeDefinition {
    node("arc_pavement", "Arc Pavement", CATEGORY)
        .desc("Traditional European fan-shaped and arched cobblestone masonry pavement.")
        .float("scale", "Scale", 6.0, 1.0, 32.0)
        .int("rings", "Arched Rings", 8, 2, 32)
        .float("mortar_width", "Mortar Width", 0.05, 0.0, 0.2)
        .float("bevel", "Stone Bevel", 0.1, 0.01, 0.5)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_arc_pavement)
}

pub(crate) fn brick_generator_definition() -> NodeDefinition {
    node("brick_generator", "Brick Generator", CATEGORY)
        .desc("Advanced parametric brick masonry generator with bond patterns, mortar gaps, bevels, and corner wear.")
        .choice("pattern", "Bond Pattern", &["Running Bond", "Stack Bond", "English Bond", "Flemish Bond", "Herringbone"], 0)
        .int("x_count", "X Count", 4, 1, 64)
        .int("y_count", "Y Count", 8, 1, 64)
        .float("gap", "Mortar Gap", 0.04, 0.0, 0.2)
        .float("bevel", "Bevel Width", 0.08, 0.001, 0.4)
        .float("luminance_random", "Luminance Random", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_brick_generator)
}

pub(crate) fn weave_generator_definition() -> NodeDefinition {
    node("weave_generator", "Weave Generator", CATEGORY)
        .desc("Parametric fabric weave pattern generator with yarn counts, thread profiles, and weave styles.")
        .choice("weave_style", "Weave Style", &["Plain 1x1", "Twill 2x2", "Basket 2x2", "Satin 4x1"], 0)
        .int("x_count", "Warp Threads", 12, 1, 64)
        .int("y_count", "Weft Threads", 12, 1, 64)
        .float("yarn_width", "Yarn Width", 0.9, 0.1, 1.0)
        .float("yarn_roundness", "Yarn Roundness", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook_weave_generator)
}

pub(crate) fn tile_random_2_definition() -> NodeDefinition {
    node("tile_random_2", "Tile Random 2", CATEGORY)
        .desc("Distributes two alternate pattern inputs across randomized grid cells with rotation and offsets.")
        .color_in("pattern_1", "Pattern 1")
        .primary_input("pattern_1")
        .color_in("pattern_2", "Pattern 2")
        .int("x_amount", "X Amount", 4, 1, 32)
        .int("y_amount", "Y Amount", 4, 1, 32)
        .float("random_rotation", "Random Rotation", 0.0, 0.0, 1.0)
        .float("random_offset", "Random Offset", 0.0, 0.0, 0.5)
        .float("pattern_ratio", "Pattern 2 Ratio", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .color_out()
        .cook(cook_tile_random_2)
}

fn cook_alveolus(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let wall_th = context.f("wall_thickness").clamp(0.01, 0.45);
    let depth = context.f("depth").clamp(0.05, 1.0);

    let sqrt3 = 3.0f32.sqrt();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale * sqrt3;
        let y = v * scale * 1.5;

        let hx = (x / sqrt3).round();
        let hy = y.round();

        let dx = (x - hx * sqrt3).abs();
        let dy = (y - hy).abs();

        let dist_to_hex = dx * 0.5 + dy * (sqrt3 * 0.5);
        let dist = dist_to_hex.max(dx);

        let cavity_r = 0.5 - wall_th;
        let val = if dist < cavity_r {
            let norm_d = dist / cavity_r;
            // Spherical concave cup
            1.0 - (1.0 - norm_d * norm_d).sqrt() * depth
        } else {
            1.0
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_arc_pavement(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 32.0);
    let rings = context.i("rings").clamp(2, 32) as f32;
    let mortar = context.f("mortar_width").clamp(0.001, 0.3);
    let bevel = context.f("bevel").clamp(0.001, 0.5);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        // Arched fan tiling
        let cx = (u * scale).rem_euclid(1.0) - 0.5;
        let cy = (v * scale).rem_euclid(1.0);

        let r = (cx * cx + cy * cy).sqrt();
        let theta = cy.atan2(cx);

        let ring_idx = (r * rings).floor() as i32;
        let ring_t = (r * rings).fract();

        let stone_count = (ring_idx + 1) * 3 + 4;
        let stone_idx = (theta * (stone_count as f32) / PI).floor() as i32;
        let stone_t = (theta * (stone_count as f32) / PI).fract();

        let d_edge_r = ring_t.min(1.0 - ring_t);
        let d_edge_theta = stone_t.min(1.0 - stone_t);
        let edge = d_edge_r.min(d_edge_theta);

        let h0 = (ring_idx as u32).wrapping_mul(0x85eb_ca6b) ^ (stone_idx as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
        let rand_lum = (crate::random::hash(h0) as f32 / 4_294_967_295.0) * 0.2;

        let val = if edge < mortar {
            0.0
        } else {
            let b = ((edge - mortar) / bevel).clamp(0.0, 1.0);
            (b * 0.8 + 0.2 - rand_lum).clamp(0.0, 1.0)
        };

        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_brick_generator(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let pattern = context.i("pattern");
    let nx = context.i("x_count").clamp(1, 64) as f32;
    let ny = context.i("y_count").clamp(1, 64) as f32;
    let gap = context.f("gap").clamp(0.0, 0.3);
    let bevel = context.f("bevel").clamp(0.001, 0.4);
    let lum_rand = context.f("luminance_random").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let cy = (v * ny).floor() as i64;
        let offset = match pattern {
            // Running Bond
            0 => (cy.rem_euclid(2) as f32) * 0.5,
            // English Bond (alternating headers and stretchers)
            2 => (cy.rem_euclid(2) as f32) * 0.25,
            // Flemish Bond
            3 => (cy.rem_euclid(2) as f32) * 0.33,
            // Stack Bond & others
            _ => 0.0,
        };

        let u_shift = u + offset / nx;
        let cx = (u_shift * nx).floor() as i64;

        let cell_u = (u_shift * nx).fract();
        let cell_v = (v * ny).fract();

        let du = cell_u.min(1.0 - cell_u);
        let dv = cell_v.min(1.0 - cell_v);
        let dist = du.min(dv);

        let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
        let rand_val = (crate::random::hash(h0) as f32 / 4_294_967_295.0 - 0.5) * lum_rand;

        let val = if dist < gap {
            0.0
        } else {
            let b = ((dist - gap) / bevel).clamp(0.0, 1.0);
            (b + rand_val).clamp(0.0, 1.0)
        };

        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_weave_generator(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let style = context.i("weave_style");
    let nx = context.i("x_count").clamp(1, 64) as f32;
    let ny = context.i("y_count").clamp(1, 64) as f32;
    let width = context.f("yarn_width").clamp(0.1, 1.0);

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let ix = (u * nx).floor() as i32;
        let iy = (v * ny).floor() as i32;

        let on_top = match style {
            1 => (ix.rem_euclid(4) + iy.rem_euclid(4)).rem_euclid(4) < 2,
            2 => (ix.div_euclid(2)).rem_euclid(2) == (iy.div_euclid(2)).rem_euclid(2),
            _ => (ix + iy).rem_euclid(2) == 0,
        };

        let cell_x = (u * nx).fract() - 0.5;
        let cell_y = (v * ny).fract() - 0.5;

        let half_w = width * 0.5;
        let (pos, perp) = if on_top { (cell_y, cell_x) } else { (cell_x, cell_y) };

        if perp.abs() > half_w {
            [0.1, 0.0, 0.0, 1.0]
        } else {
            let profile = (1.0 - (perp / half_w).powi(2)).max(0.0).sqrt();
            let wave = (pos * PI).cos() * 0.3 + 0.7;
            let val = (profile * wave).clamp(0.0, 1.0);
            [val, 0.0, 0.0, 1.0]
        }
    })])
}

fn cook_tile_random_2(context: &CookContext<'_>) -> CookResult {
    let p1 = context.any_or(0, Kind::Color, [1.0, 1.0, 1.0, 1.0]);
    let p2 = context.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]);

    let nx = context.i("x_amount").clamp(1, 64) as f32;
    let ny = context.i("y_amount").clamp(1, 64) as f32;
    let rot_rand = context.f("random_rotation").clamp(0.0, 1.0);
    let off_rand = context.f("random_offset").clamp(0.0, 0.5);
    let ratio = context.f("pattern_ratio").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let ix = (u * nx).floor() as i64;
        let iy = (v * ny).floor() as i64;

        let h0 = (ix as u32).wrapping_mul(0x85eb_ca6b) ^ (iy as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
        let h1 = crate::random::hash(h0);
        let h2 = crate::random::hash(h1);
        let h3 = crate::random::hash(h2);

        let use_p2 = (h1 as f32 / 4_294_967_295.0) < ratio;
        let angle = ((h2 % 4) as f32) * (PI * 0.5) * rot_rand;
        let off_x = (h3 as f32 / 4_294_967_295.0 - 0.5) * off_rand;
        let off_y = (crate::random::hash(h3) as f32 / 4_294_967_295.0 - 0.5) * off_rand;

        let cell_u = (u * nx).fract() - 0.5 - off_x;
        let cell_v = (v * ny).fract() - 0.5 - off_y;

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let local_u = cell_u * cos_a - cell_v * sin_a + 0.5;
        let local_v = cell_u * sin_a + cell_v * cos_a + 0.5;

        if (0.0..=1.0).contains(&local_u) && (0.0..=1.0).contains(&local_v) {
            if use_p2 { p2.sample_axes(local_u, local_v, [false, false]) } else { p1.sample_axes(local_u, local_v, [false, false]) }
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    })])
}
