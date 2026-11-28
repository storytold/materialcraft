use materialcraft_core::{PatternGraph, PatternNodeKind, Value};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::collections::BTreeMap;

fn render(region: [f32; 4], tiling: [f32; 4], tile: bool) -> materialcraft_nodes::CookResult {
    render_stamp(region, tiling, if tile { 3 } else { 0 }, [0.0; 2], [0.5; 2])
}

fn render_stamp(region: [f32; 4], tiling: [f32; 4], axes: i32, offset: [f32; 2], size: [f32; 2]) -> materialcraft_nodes::CookResult {
    render_culled_stamp(region, tiling, axes, offset, size, false)
}

fn render_culled_stamp(region: [f32; 4], tiling: [f32; 4], axes: i32, offset: [f32; 2], size: [f32; 2], cull: bool) -> materialcraft_nodes::CookResult {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[0].parameters.insert("patternsize".into(), Value::Float2(size));
    graph.nodes[0].parameters.insert("frameoffset".into(), Value::Float2(offset));
    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("cull_outside".into(), Value::Bool(cull)),
            ("render_region".into(), Value::Float4(region)),
            ("tiling_region".into(), Value::Float4(tiling)),
        ]),
    );
    (definition.cook)(&CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 64,
        h: 64,
        seed: 0,
        tile: axes == 3,
        inputs: &[],
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::from([("$tiling".into(), Value::Int(axes))]),
    })
}

#[test]
fn render_region_samples_a_cropped_area() {
    let identity = [0.0, 1.0, 0.0, 1.0];
    let full = render(identity, identity, false).unwrap().remove(0);
    let crop = render([0.25, 0.75, 0.25, 0.75], identity, false).unwrap().remove(0);

    assert_eq!(full.mean()[0], 0.25);
    assert!(crop.data().iter().all(|value| *value == 1.0));
}

#[test]
fn expanding_the_render_window_preserves_the_tiling_region() {
    let region = [0.0, 2.0, 0.0, 1.0];
    let repeated = render(region, [0.0, 1.0, 0.0, 1.0], true).unwrap().remove(0);
    let wider = render(region, region, true).unwrap().remove(0);
    let untiled = render(region, [0.0, 1.0, 0.0, 1.0], false).unwrap().remove(0);

    assert_eq!(repeated.get(48, 32, false)[0], 0.0);
    assert_eq!(wider.get(48, 32, false)[0], 0.0);
    assert_eq!(wider.data(), untiled.data());
    assert_eq!(repeated.mean()[0], 0.125);
    assert_eq!(wider.mean()[0], 0.125);
}

#[test]
fn wide_tiling_regions_place_and_wrap_stamps_at_the_authored_period() {
    for vertical in [false, true] {
        let region = if vertical { [-15.5, 16.5, -31.5, 32.5] } else { [-31.5, 32.5, -15.5, 16.5] };
        for (center, size, columns, rows) in
            [([26.0, 2.0], [3.0, 4.0], 56..=58, 31..=38), ([-24.0, -3.0], [3.0, 5.0], 6..=8, 20..=29), ([35.0, 4.0], [4.0, 3.0], 1..=4, 36..=41)]
        {
            let orient = |[x, y]: [f32; 2]| if vertical { [y, x] } else { [x, y] };
            let offset = orient(center).map(|v| v - 0.5);
            let image = render_stamp(region, region, 3, offset, orient(size)).unwrap().remove(0);
            for y in 0..64 {
                for x in 0..64 {
                    let (column, row) = if vertical { (y, x) } else { (x, y) };
                    let expected = if columns.contains(&column) && rows.contains(&row) { 1.0 } else { 0.0 };
                    assert_eq!(image.get(x, y, false)[0], expected, "center {center:?}, pixel ({x},{y}), vertical {vertical}");
                }
            }
        }
    }
}

#[test]
fn a_stamp_repeats_on_each_enabled_axis_inside_the_tiling_region() {
    let unit = [0.0, 1.0, 0.0, 1.0];
    for (region, tiling, counts) in [(unit, unit, [27, 324, 72, 864]), ([-0.25, 1.25, 0.0, 1.0], [0.125, 0.875, 0.25, 0.75], [117, 216, 312, 576])] {
        for (mask, count) in counts.into_iter().enumerate() {
            let image = render_stamp(region, tiling, mask as i32, [-0.734375, -0.546875], [0.5625, 0.375]).unwrap().remove(0);
            assert_eq!(image.mean()[0] * 4096.0, count as f32, "tiling mask {mask}");
        }
    }
}

#[test]
fn a_stamp_that_only_touches_the_tiling_interval_contributes_no_copy() {
    let render = [0.0, 1.0, 0.0, 1.0];
    let tile = [0.25, 0.75, 0.125, 0.875];
    for (offset, excluded, included) in [([-0.375, 0.375], [8, 56], [40, 56]), ([0.375, -0.375], [56, 8], [24, 8])] {
        let image = render_stamp(render, tile, 1, offset, [0.25, 0.5]).unwrap().remove(0);
        assert_eq!(image.get(excluded[0], excluded[1], false)[0], 0.0);
        assert_eq!(image.get(included[0], included[1], false)[0], 1.0);
    }
}

#[test]
fn degenerate_regions_and_excessive_repetition_return_errors() {
    let identity = [0.0, 1.0, 0.0, 1.0];

    assert!(render([0.0; 4], identity, true).is_err());
    assert!(render(identity, [0.0; 4], true).is_err());
    assert!(render(identity, [0.0, 1e-9, 0.0, 1e-9], true).is_err());
}

#[test]
fn center_culling_uses_both_original_coordinates_before_repetition() {
    let unit = [0.0, 1.0, 0.0, 1.0];
    let region = [0.25, 0.75, 0.125, 0.875];
    for mask in 0..=3 {
        for (center, visible) in [
            ([0.25, 0.625], true),
            ([0.24902344, 0.625], false),
            ([0.75, 0.625], false),
            ([0.625, 0.125], true),
            ([0.625, 0.875], false),
            ([1.0625, 0.4375], false),
        ] {
            let image = render_culled_stamp(unit, region, mask, center.map(|v| v - 0.5), [0.484375, 0.40625], true).unwrap().remove(0);
            assert_eq!(image.mean()[0] > 0.0, visible, "mask {mask}, center {center:?}");
        }
    }
}

#[test]
fn a_pattern_background_filters_stored_levels_and_requires_matching_channels() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;
    use materialcraft_nodes::output_precision;

    let definition = find("fx_map").unwrap();
    let field = Image::gray(32, 32, |x, y| (((17 * x + 113 * y + 59 * x * y + 19) * 7919) % 65536) as f32 / 65535.0);
    let images = [Some(output_precision::apply(field, OutputFormat::U16))];
    for color in [false, true] {
        let params = resolve_params(
            definition,
            &BTreeMap::from([("color_mode".into(), Value::Bool(color)), ("background".into(), Value::Float4([0.125, 0.375, 0.8125, 0.25]))]),
        );
        let ctx = CookContext {
            w: 8,
            h: 8,
            seed: 15709,
            format: OutputFormat::U8,
            tile: true,
            vars: &BTreeMap::new(),
            resource: &|_| None,
            input_formats: &[OutputFormat::U16],
            inputs: &images,
            params: &params,
        };
        let image = output_precision::apply((definition.cook)(&ctx).unwrap().remove(0), OutputFormat::U8);
        if color {
            assert_eq!(image.kind(), Kind::Color);
            assert!(image.data().iter().all(|&v| v == 0.0));
        } else {
            assert_eq!([2, 5].map(|x| (255.0 * image.get_gray(x, 0, false)).round() as u8), [146, 140]);
        }
    }
}
