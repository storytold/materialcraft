use std::collections::BTreeMap;

use craft_texture::{Image, Kind};
use materialcraft_core::Value;
use materialcraft_nodes::{CookContext, find, resolve_params};

fn cook(mask: Image, source: Option<Image>, tile: bool, combine: bool, distance: f32) -> Image {
    cook_metric(mask, source, tile, combine, distance, 0)
}

fn cook_metric(mask: Image, source: Option<Image>, tile: bool, combine: bool, distance: f32, metric: u32) -> Image {
    let color = source.as_ref().is_some_and(|image| !image.is_gray());
    cook_mode(mask, source, tile, combine, distance, metric, color)
}

fn cook_mode(mask: Image, source: Option<Image>, tile: bool, combine: bool, distance: f32, metric: u32, color: bool) -> Image {
    let values = BTreeMap::from([
        ("distance".into(), Value::Float(distance)),
        ("combine".into(), Value::Bool(combine)),
        ("metric".into(), Value::Enum(metric)),
        ("color_mode".into(), Value::Bool(color)),
    ]);
    cook_with_vars(mask, source, tile, values, BTreeMap::new())
}

fn cook_with_vars(mask: Image, source: Option<Image>, tile: bool, values: BTreeMap<String, Value>, vars: BTreeMap<String, Value>) -> Image {
    let def = find("distance").unwrap();
    let params = resolve_params(def, &values);
    let (w, h) = (mask.width(), mask.height());
    let inputs = [Some(mask), source];
    let resource = |_: &str| None;
    let ctx = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w,
        h,
        seed: 0,
        tile,
        inputs: &inputs,
        params: &params,
        resource: &resource,
        vars: &vars,
    };
    (def.cook)(&ctx).unwrap().remove(0)
}

#[test]
fn distance_color_selection_controls_empty_sources_and_channel_mismatches() {
    let mask = Image::gray(32, 32, |x, y| if (x, y) == (16, 16) { 1.0 } else { 0.0 });
    let gray = cook(mask.clone(), None, true, true, 31.0);
    for combine in [true, false] {
        let color = cook_mode(mask.clone(), None, true, combine, 31.0, 0, true);
        assert_eq!(color.kind(), Kind::Color);
        assert_eq!(color.data(), gray.to_color().data());
    }
    for (source_kind, color) in [(Kind::Color, false), (Kind::Gray, true)] {
        let source = Image::filled(32, 32, source_kind, [0.75; 4]);
        let output = cook_mode(mask.clone(), Some(source), true, true, 31.0, 0, color);
        assert_eq!(output.kind(), if color { Kind::Color } else { Kind::Gray });
        assert!(output.data().iter().all(|v| v.to_bits() == 0));
    }
}

#[test]
fn tiled_distance_spreads_source_across_stationary_gradient() {
    let mask = Image::gray(32, 32, |x, y| if x == 0 && y == 0 { 1.0 } else { 0.0 });
    let output = cook(mask.clone(), Some(mask), true, false, 256.0);

    // A zero slope at the only mask pixel must not stop the walk on background
    assert_eq!(output.get_gray(16, 16, false), 1.0);
    assert!(output.data().iter().all(|&v| v == 1.0));
}

#[test]
fn distance_spreads_source_beyond_the_old_walk_limit() {
    let mask = Image::gray(512, 512, |x, y| if x == 0 && y == 0 { 1.0 } else { 0.0 });
    let output = cook(mask.clone(), Some(mask), false, false, 256.0);
    assert_eq!(output.get_gray(511, 0, false), 1.0);
}

#[test]
fn distance_spreads_rgb_and_attenuates_the_alpha_channel() {
    let mask = Image::gray(32, 32, |x, y| if x == 16 && y == 16 { 0.5 } else { 0.0 });
    let source =
        Image::generate(32, 32, Kind::Color, |x, y| std::array::from_fn(|c| ((13 * x + 7 * y + 5 * x * y + 19 * c as u32) % 127) as f32 / 64.0 - 0.375));
    let output = cook(mask.clone(), Some(source.clone()), true, true, 31.0);
    assert_eq!(output.get(17, 16, false).map(f32::to_bits), [0x3f50_0000, 0x3f8e_0000, 0x3fb4_0000, 0xbe55_a8ec]);
    assert_eq!(output.get(0, 0, false), [-0.375, -0.078125, 0.21875, 0.0]);
    let copy = cook(mask, Some(source.clone()), true, false, 31.0);
    assert_eq!(copy.get(17, 16, false), source.get(16, 16, false));
    assert_eq!(copy.get(0, 0, false), source.get(0, 0, false));
}

#[test]
fn alternative_distance_metrics_use_the_quantized_site_center() {
    let mask = Image::gray(64, 128, |x, y| if (x, y) == (1, 2) { 1.0 } else { 0.0 });
    for (metric, bits) in [(1, 0x3f29_5041u32), (2, 0x3f54_a816)] {
        let image = cook_metric(mask.clone(), None, false, true, 47.25, metric);
        assert_eq!(image.get_gray(3, 4, false).to_bits(), bits);
    }
}

#[test]
fn distance_keeps_the_current_source_texel_at_the_radius() {
    let mask = Image::gray(32, 32, |x, y| if (x, y) == (16, 16) { 1.0 } else { 0.0 });
    let source = Image::gray(32, 32, |x, y| if (x, y) == (16, 16) { 0.8125 } else { -0.109375 });
    for tile in [false, true] {
        for (combine, expected) in [(false, -0.109375f32), (true, -0.0f32)] {
            let image = cook_metric(mask.clone(), Some(source.clone()), tile, combine, 16.0, 1);
            assert_eq!(image.get_gray(14, 16, false).to_bits(), expected.to_bits());
            assert_eq!(image.get_gray(16, 16, false), 0.8125);
        }
    }
}

#[test]
fn zero_distance_and_a_positive_subpixel_distance_have_distinct_site_values() {
    let mask = Image::gray(32, 32, |x, y| if (x, y) == (1, 2) { 1.0 } else { 0.0 });
    for (strength, expected) in [(0.0, 0.0), (0.125, 1.0), (f32::MIN_POSITIVE, 1.0)] {
        let image = cook(mask.clone(), None, true, true, strength);
        assert_eq!(image.get_gray(1, 2, false), expected);
        assert_eq!(image.get_gray(2, 2, false), 0.0);
    }
}

#[test]
fn distance_handles_empty_full_masks_and_no_source() {
    for tile in [false, true] {
        let empty = Image::filled(7, 5, Kind::Gray, [0.0; 4]);
        let full = Image::filled(7, 5, Kind::Gray, [1.0; 4]);
        let source = Image::generate(7, 5, Kind::Color, |x, y| [x as f32 / 7.0, y as f32 / 5.0, 0.4, 0.7]);

        let background = cook(empty.clone(), Some(source.clone()), tile, false, 256.0);
        assert_eq!(background.get(3, 2, false), if tile { source.get(3, 2, false) } else { source.get(0, 0, false) });
        assert_eq!(cook(full, Some(source), tile, true, 256.0).get(3, 2, false), [3.0 / 7.0, 2.0 / 5.0, 0.4, 0.7]);
        assert!(cook(empty, None, tile, true, 256.0).data().iter().all(|&v| v == 0.0));

        let mask = Image::gray(32, 32, |x, y| if (x, y) == (16, 16) { 0.5 } else { 0.499 });
        let ramp = cook(mask, None, tile, false, 16.0);
        assert_eq!(ramp.get_gray(16, 16, false), 1.0);
        assert_eq!(ramp.get_gray(17, 16, false).to_bits(), 0x3eff_f080);
        assert_eq!(ramp.get_gray(19, 16, false), 0.0);
    }
}

#[test]
fn mask_distance_uses_stored_site_centers_and_the_shorter_dimension() {
    let mask = Image::gray(128, 64, |x, y| if (x, y) == (64, 32) { 1.0 } else { 0.0 });
    let ramp = cook(mask, None, true, true, 256.0 * std::f32::consts::SQRT_2);
    for (x, y, bits) in [(65, 32, 0x3f7d_2b39), (64, 33, 0x3f7d_2b93), (65, 33, 0x3f7b_ff42), (63, 32, 0x3f7d_2ca0)] {
        assert_eq!(ramp.get_gray(x, y, false).to_bits(), bits);
    }
}

#[test]
fn sparse_mask_distance_preserves_the_sites_selected_during_propagation() {
    let mask = Image::gray(128, 128, |x, y| if (13 * x + 23 * y + 11 * x * y) % 109 == 0 { 1.0 } else { 0.0 });
    let ramp = cook(mask, None, true, true, 17.375);
    for (x, y, bits) in [(99, 0, 0x3e86_a510), (64, 30, 0x3e26_0066), (39, 55, 0x3e25_ccdc)] {
        assert_eq!(ramp.get_gray(x, y, false).to_bits(), bits);
    }
}

fn gray_field(mask: Image, source: Option<Image>, tiling: i32, ratio: i32, combine: bool, strength: f32) -> Image {
    let values = BTreeMap::from([("distance".into(), Value::Float(strength)), ("combine".into(), Value::Bool(combine))]);
    let vars = BTreeMap::from([("$pixelratio".into(), Value::Int(ratio)), ("$tiling".into(), Value::Int(tiling))]);
    cook_with_vars(mask, source, true, values, vars)
}

fn distance_neighbors(w: u32, h: u32, site: [u32; 2], tiling: i32, ratio: i32, strength: f32) -> [u32; 2] {
    let mask = Image::gray(w, h, |x, y| if [x, y] == site { 1.0 } else { 0.0 });
    let result = gray_field(mask, None, tiling, ratio, true, strength);
    let [x, y] = site.map(i64::from);
    [result.get_gray(x + 1, y, false), result.get_gray(x, y + 1, false)].map(f32::to_bits)
}

#[test]
fn rectangular_distance_uses_the_resolved_pixel_ratio_on_both_axes() {
    for (w, h, ratio, strength, expected) in [
        (32, 16, 0, 64.0, [0x3f5f_fe10, 0x3f3f_fe20]),
        (32, 16, 1, 64.0, [0x3f3f_fc20, 0x3f3f_fe20]),
        (16, 64, 0, 64.0, [0x3f3f_fe20, 0x3f6f_fe08]),
        (16, 64, 1, 64.0, [0x3f3f_fe20, 0x3f3f_f820]),
        (16, 64, 0, 7.25, [0, 0x3ee5_61a6]),
    ] {
        assert_eq!(distance_neighbors(w, h, [w / 2, h / 2], 3, ratio, strength), expected, "{w} x {h}, ratio {ratio}, strength {strength}");
    }
}

#[test]
fn distance_rounds_site_coordinates_once_at_word_precision() {
    assert_eq!(distance_neighbors(256, 16, [128, 8], 3, 1, 87.625), [0x3f51_29ff, 0x3f51_3ffa]);
    assert_eq!(distance_neighbors(16, 256, [8, 128], 3, 1, 87.625), [0x3f51_3ffa, 0x3f51_29ff]);
}

#[test]
fn distance_measures_off_center_sites_from_the_image_midpoint() {
    for tiling in 0..4 {
        let x = if tiling & 1 == 0 { 0x3f68_a0d2 } else { 0x3f68_a0de };
        let y = if tiling & 2 == 0 { 0x3f72_fc9f } else { 0x3f72_fca5 };
        assert_eq!(distance_neighbors(512, 32, [1, 30], tiling, 1, 87.625), [x, 0x3f68_a08c]);
        assert_eq!(distance_neighbors(32, 512, [30, 1], tiling, 1, 157.375), [0x3f72_fc78, y]);
    }
}

fn spread_sites(w: u32, h: u32, ratio: i32, sites: &[[u32; 2]]) -> Image {
    let mask = Image::gray(w, h, |x, y| if sites.contains(&[x, y]) { 1.0 } else { 0.0 });
    let source = Image::gray(w, h, |x, y| sites.iter().position(|s| *s == [x, y]).map_or(0.0625, |i| (i + 1) as f32 / 8.0));
    gray_field(mask, Some(source), 3, ratio, false, 256.0)
}

#[test]
fn distance_projects_each_jump_onto_both_image_axes() {
    for (w, h, ratio, sites, sample, expected) in [
        (64, 16, 0, &[[60, 2], [14, 13]][..], [28, 5], 0.125),
        (32, 16, 1, &[[9, 13], [21, 5], [7, 1]][..], [31, 10], 0.125),
        (32, 64, 0, &[[7, 59], [8, 31], [12, 37]][..], [18, 12], 0.375),
    ] {
        let result = spread_sites(w, h, ratio, sites);
        assert_eq!(result.get_gray(sample[0], sample[1], false), expected);
    }
}

#[test]
fn distance_preserves_the_first_neighbor_at_equal_distances() {
    let output = spread_sites(32, 32, 0, &[[19, 20], [20, 19], [1, 10]]);
    assert_eq!(output.get_gray(12, 12, false), 0.125);
    let output = spread_sites(128, 128, 0, &[[56, 64], [63, 71]]);
    assert_eq!(output.get_gray(119, 8, false), 0.125);
}

#[test]
fn distance_resolves_tied_sources_in_axis_pairs_before_diagonals() {
    let mask = Image::gray(32, 32, |x, y| if (x % 3 == 0 && y % 3 == 0) || (x + 2 * y) % 7 == 3 { 1.0 } else { 0.0 });
    let source = Image::gray(32, 32, |x, y| (1 + x + 32 * y) as f32 / 1024.0);
    for (metric, target, site) in [(1, [30, 8], [29, 8]), (2, [14, 0], [15, 0]), (2, [19, 0], [18, 0])] {
        let result = cook_metric(mask.clone(), Some(source.clone()), true, false, 256.0, metric);
        assert_eq!(result.get_gray(target[0], target[1], false), source.get_gray(site[0], site[1], false));
    }
}
