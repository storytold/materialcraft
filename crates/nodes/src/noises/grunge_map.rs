use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("grunge_map", "Grunge Map 001", CATEGORY)
        .desc("Organic weathering and dirt map blending multi-scale gradient noise and turbulence.")
        .float("contrast", "Contrast", 1.0, 0.0, 2.0)
        .float("balance", "Balance", 0.5, 0.0, 1.0)
        .float("scale", "Scale", 4.0, 1.0, 16.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let contrast = context.f("contrast").clamp(0.0, 2.0);
    let balance = context.f("balance").clamp(0.0, 1.0);
    let scale = context.f("scale").clamp(1.0, 16.0);
    let user_seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let n1 = super::perlin::sample_perlin_fbm(u, v, scale * 2.0, 4, 0.5, 2.0, user_seed);
        let n2 = super::turbulence::sample_turbulence_fbm(u, v, scale * 4.0, 4, 0.5, user_seed.wrapping_add(101));

        let raw = n1 * 0.6 + n2 * 0.4;
        let centered = raw - balance;
        let val = (centered * (contrast * 2.0) + 0.5).clamp(0.0, 1.0);

        [val, val, val, 1.0]
    })])
}

fn cook_grunge_preset(ctx: &CookContext<'_>, seed_idx: u32) -> CookResult {
    let contrast = ctx.f("contrast").clamp(0.1, 5.0);
    let balance = ctx.f("balance").clamp(0.0, 1.0);
    let scale = ctx.f("scale").clamp(1.0, 64.0);
    let seed = (ctx.i("seed").max(0) as u32).wrapping_add(seed_idx * 1000);

    Ok(vec![Image::generate_uv(ctx.w, ctx.h, Kind::Gray, |u, v| {
        let n1 = super::perlin::sample_perlin_fbm(u, v, scale * 2.0, 4, 0.55, 2.0, seed);
        let n2 = super::turbulence::sample_turbulence_fbm(u + 0.2, v + 0.4, scale * 3.5, 4, 0.6, seed.wrapping_add(500));
        let raw = n1 * 0.6 + n2 * 0.4;
        let val = ((raw - balance) * contrast + 0.5).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

macro_rules! grunge_map {
    ($def_fn:ident, $cook_fn:ident, $id:expr, $label:expr, $desc:expr, $scale:expr, $idx:expr) => {
        pub(crate) fn $def_fn() -> NodeDefinition {
            node($id, $label, CATEGORY)
                .desc($desc)
                .float("contrast", "Contrast", 1.2, 0.1, 4.0)
                .float("balance", "Balance", 0.5, 0.0, 1.0)
                .float("scale", "Scale", $scale, 1.0, 32.0)
                .int("seed", "Random Seed", 1234, 0, 100000)
                .gray_out()
                .cook($cook_fn)
        }

        fn $cook_fn(ctx: &CookContext<'_>) -> CookResult {
            cook_grunge_preset(ctx, $idx)
        }
    };
}

grunge_map!(def_002, cook_002, "grunge_map_002", "Grunge Map 002", "High-contrast blotchy plaster and micro-pores.", 6.0, 2);
grunge_map!(def_003, cook_003, "grunge_map_003", "Grunge Map 003", "Fine dry dust and subtle chalky streaks.", 8.0, 3);
grunge_map!(def_004, cook_004, "grunge_map_004", "Grunge Map 004", "Pitted eroded stone and cellular weathering.", 5.0, 4);
grunge_map!(def_005, cook_005, "grunge_map_005", "Grunge Map 005", "Peeling paint layers and organic edge flecks.", 6.0, 5);
grunge_map!(def_006, cook_006, "grunge_map_006", "Grunge Map 006", "Water stains and drying condensation marks.", 7.0, 6);
grunge_map!(def_007, cook_007, "grunge_map_007", "Grunge Map 007", "Greasy oil smudges and fingerprint-like swirls.", 4.0, 7);
grunge_map!(def_008, cook_008, "grunge_map_008", "Grunge Map 008", "Industrial metal rust pitting and corrosion.", 9.0, 8);
grunge_map!(def_009, cook_009, "grunge_map_009", "Grunge Map 009", "Cracked dried mud flakes and crevice dirt.", 5.0, 9);
grunge_map!(def_010, cook_010, "grunge_map_010", "Grunge Map 010", "Coarse asphalt, tarmac and concrete grit.", 12.0, 10);
grunge_map!(def_011, cook_011, "grunge_map_011", "Grunge Map 011", "Delicate marble-like weathering veins.", 4.0, 11);
grunge_map!(def_012, cook_012, "grunge_map_012", "Grunge Map 012", "Heavy wall moisture and mildew spots.", 7.0, 12);
grunge_map!(def_013, cook_013, "grunge_map_013", "Grunge Map 013", "Floor scuff marks and mechanical abrasions.", 8.0, 13);
grunge_map!(def_014, cook_014, "grunge_map_014", "Grunge Map 014", "Directional rain wash and wall runoff streaks.", 6.0, 14);
grunge_map!(def_015, cook_015, "grunge_map_015", "Grunge Map 015", "Organic lichen and moss-like cluster patches.", 8.0, 15);
