use craft_texture::Image;
use materialcraft_core::{Graph, OutputFormat, Package, ParameterInheritance, Value};
use materialcraft_eval::{Cache, Options, cook};
use std::sync::Mutex;

#[test]
fn rectangular_directional_blur_uses_the_resolved_pixel_ratio() {
    let mut graph = Graph::new("Directional filter");
    graph.format = OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text("stripe".into()));
    let blur = graph.add_node("directional_blur", [100.0, 0.0]);
    graph.node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(2.4));
    graph.connect(input, "output", blur, "input").unwrap();
    let mut package = Package::new("Pixel ratio");
    package.graphs.push(graph);
    let resource = |_: &str| None;
    let mut options = Options::new(&resource);
    let cache = Mutex::new(Cache::default());

    for (size, angle) in [([5, 6], 0.25), ([6, 5], 0.0)] {
        let (w, h) = (1 << size[0], 1 << size[1]);
        package.graphs[0].size = size;
        let vertical = h > w;
        let stripe = Image::gray(w, h, |x, y| f32::from(if vertical { y == h / 2 } else { x == w / 2 }));
        options.inputs.insert("stripe".into(), stripe);
        package.graphs[0].node_mut(blur).unwrap().params.insert("angle".into(), Value::Float(angle));
        for (graph_ratio, node_ratio, expected) in [(None, None, 1), (Some(0), None, 0), (Some(0), Some(1), 1), (Some(1), Some(0), 0)] {
            let graph = &mut package.graphs[0];
            graph.pixel_ratio = graph_ratio.unwrap_or_default();
            graph.pixel_ratio_mode = if graph_ratio.is_some() { ParameterInheritance::Absolute } else { ParameterInheritance::RelativeToParent };
            let base = &mut graph.node_mut(blur).unwrap().base;
            base.pixel_ratio = node_ratio.unwrap_or_default();
            base.pixel_ratio_mode = if node_ratio.is_some() { ParameterInheritance::Absolute } else { ParameterInheritance::RelativeToInput };
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&blur].base.pixel_ratio, expected);
            let image = cooked.image(blur, 0).unwrap();
            let (x, y) = (i64::from(w / 2), i64::from(h / 2));
            let center = image.get_gray(x, y, false).to_bits();
            let neighbor = image.get_gray(x + i64::from(!vertical), y + i64::from(vertical), false).to_bits();
            let samples = if expected == 1 {
                [1.0f32.to_bits(), 0]
            } else if vertical {
                [0x3f55_5555, 0x3daa_aaac]
            } else {
                [0x3f55_5555, 0x3daa_aaad]
            };
            assert_eq!([center, neighbor], samples, "{size:?} {graph_ratio:?} {node_ratio:?}");
        }
    }

    // The vertical extent is zero here. The transverse projection still reaches the adjacent column.
    package.graphs[0].node_mut(blur).unwrap().params.insert("angle".into(), Value::Float(0.25));
    let cooked = cook(&package, 0, &options, &cache);
    assert_eq!(cooked.image(blur, 0).unwrap().get_gray(33, 16, false).to_bits(), 0x3216_30f4);

    for (size, strength, expected) in [([5, 6], 4.8, 0x3f52_d2b2), ([6, 5], 9.6, 0x3f0b_996e)] {
        package.graphs[0].size = size;
        let params = &mut package.graphs[0].node_mut(blur).unwrap().params;
        params.insert("angle".into(), Value::Float(0.0625));
        params.insert("intensity".into(), Value::Float(strength));
        let field = Image::gray(1 << size[0], 1 << size[1], |x, y| ((47 * x + 73 * y + 13 * x * y + 79) % 509) as f32 / 508.0);
        options.inputs.insert("stripe".into(), field);
        let cooked = cook(&package, 0, &options, &cache);
        assert_eq!(cooked.image(blur, 0).unwrap().get_gray(7, 13, false).to_bits(), expected);
    }
}

#[test]
fn atomic_filters_wrap_only_the_requested_axes_and_invalidate_cache() {
    for operation in ["blur", "directional_blur", "sharpen", "distance"] {
        let mut graph = Graph::new("Filter boundaries");
        graph.size = [4; 2];
        graph.format = OutputFormat::F32;
        graph.tiling_mode = ParameterInheritance::Absolute;
        let input = graph.add_node("input_grayscale", [0.0; 2]);
        graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text("source".into()));
        let filter = graph.add_node(operation, [100.0, 0.0]);
        let (parameter, port, center, background) = if operation == "distance" { ("distance", "mask", 1.0, 0.0) } else { ("intensity", "input", 0.5625, 0.5) };
        let strength = match operation {
            "distance" => 48.0,
            "sharpen" => 1.0,
            _ => 16.0,
        };
        graph.node_mut(filter).unwrap().params.insert(parameter.into(), Value::Float(strength));
        graph.connect(input, "output", filter, port).unwrap();
        let mut package = Package::new("Filter boundaries");
        package.graphs.push(graph);
        let resource = |_: &str| None;
        let mut options = Options::new(&resource);
        options.inputs.insert("source".into(), Image::gray(16, 16, |x, y| if x == 0 && y == 0 { center } else { background }));
        let cache = Mutex::new(Cache::default());

        for tiling in [[false, false], [true, false], [false, true], [true, true]] {
            package.graphs[0].tiling = tiling;

            let cooked = cook(&package, 0, &options, &cache);
            let image = cooked.image(filter, 0).unwrap();

            assert_eq!(cooked.nodes[&filter].error, None);
            assert!(!cooked.nodes[&filter].cached);
            assert_eq!(image.get_gray(15, 0, false) != background, tiling[0], "{operation}, x, {tiling:?}");
            assert_eq!(image.get_gray(0, 15, false) != background, tiling[1] && operation != "directional_blur", "{operation}, y, {tiling:?}");

            let cached = cook(&package, 0, &options, &cache);

            assert!(cached.nodes[&filter].cached);
        }
    }
}

#[test]
fn directional_warp_uses_each_graph_tiling_axis() {
    let mut graph = Graph::new("Warp boundaries");
    graph.size = [4; 2];
    graph.format = OutputFormat::F32;
    graph.tiling_mode = ParameterInheritance::Absolute;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text("field".into()));
    let mask = graph.add_node("input_grayscale", [0.0, 50.0]);
    graph.node_mut(mask).unwrap().params.insert("identifier".into(), Value::Text("mask".into()));
    let warp = graph.add_node("directional_warp", [100.0, 0.0]);
    graph.node_mut(warp).unwrap().params.insert("intensity".into(), Value::Float(16.0));
    graph.connect(input, "output", warp, "input").unwrap();
    graph.connect(mask, "output", warp, "intensity_input").unwrap();
    let mut package = Package::new("Warp boundaries");
    package.graphs.push(graph);
    let resource = |_: &str| None;
    let mut options = Options::new(&resource);
    options.inputs.insert("field".into(), Image::gray(16, 16, |x, y| f32::from(x == 0 && y == 0)));
    options.inputs.insert("mask".into(), Image::gray(16, 16, |_, _| 1.0));
    let cache = Mutex::new(Cache::default());

    for tiling in [[false, false], [true, false], [false, true], [true, true]] {
        package.graphs[0].tiling = tiling;
        for (axis, angle, x, y) in [(0, 0.0, 15, 0), (1, 0.25, 0, 15)] {
            package.graphs[0].node_mut(warp).unwrap().params.insert("angle".into(), Value::Float(angle));
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&warp].error, None);
            let image = cooked.image(warp, 0).unwrap();
            assert_eq!(image.get_gray(x, y, false), f32::from(tiling[axis]), "tiling {tiling:?}, angle {angle}");
        }
    }
}
