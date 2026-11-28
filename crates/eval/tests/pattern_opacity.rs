use materialcraft_core::{FunctionGraph, Graph, OutputFormat, Package, PatternGraph, PatternNodeKind, Value, ValueType};
use materialcraft_eval::{Cache, Options, cook};
use std::sync::Mutex;

#[test]
fn pattern_seeds_distinguish_published_integers_from_assignments() {
    let mut graph = Graph::new("Integer seed storage");
    graph.size = [4; 2];
    graph.format = OutputFormat::F32;
    let stamp = graph.add_node("fx_map", [0.0; 2]);
    let node = graph.node_mut(stamp).unwrap();
    node.params.insert("color_mode".into(), Value::Bool(false));
    node.params.insert("roughness".into(), Value::Float(0.0));
    node.params.insert("background".into(), Value::Color([0.0, 0.0, 0.0, 1.0]));
    let mut pattern = PatternGraph::with_root(PatternNodeKind::Iterate);
    let inner = pattern.add(PatternNodeKind::Iterate).unwrap();
    let leaf = pattern.add(PatternNodeKind::Quadrant).unwrap();
    pattern.nodes[0].children[1] = Some(inner);
    pattern.nodes[1].children[1] = Some(leaf);
    let fields = &mut pattern.nodes[1].parameters;
    fields.insert("numberadded".into(), Value::Int(1));
    fields.insert("randominherited".into(), Value::Bool(false));
    fields.insert("randomseed".into(), Value::Function(Box::new(FunctionGraph::input("seed_value", ValueType::Integer))));
    let mut opacity = FunctionGraph::default();
    let one = opacity.add_constant(Value::Float(1.0));
    opacity.output = Some(opacity.add("rand", &[one], None));
    let fields = &mut pattern.nodes[2].parameters;
    fields.insert("patterntype".into(), Value::Int(2));
    fields.insert("blendingmode".into(), Value::Int(0));
    fields.insert("opacity".into(), Value::Function(Box::new(opacity)));
    let mut package = Package::new("Integer seed storage");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());

    let write_seed = |result| {
        let mut f = FunctionGraph::default();
        let value = f.add_constant(Value::Int(17));
        let store = f.add("set", &[value], Some(Value::Text("seed_value".into())));
        let result = f.add_constant(result);
        f.output = Some(f.add("sequence", &[store, result], None));
        f
    };
    for (scope, expected) in [("input", 0.50918543), ("opacity", 0.26489496), ("iterate", 0.26489496), ("input", 0.50918543)] {
        let mut options = Options::new(&|_| None);
        let node = package.graphs[0].node_mut(stamp).unwrap();
        node.functions.remove("opacity");
        let count = match scope {
            "input" => {
                options.overrides.insert("seed_value".into(), Value::Int(17));
                Value::Int(1)
            }
            "opacity" => {
                node.functions.insert("opacity".into(), write_seed(Value::Float(1.0)));
                Value::Int(1)
            }
            _ => Value::Function(Box::new(write_seed(Value::Int(1)))),
        };
        pattern.nodes[0].parameters.insert("numberadded".into(), count);
        node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern.clone())));
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&stamp].error, None);
        assert_eq!(result.image(stamp, 0).unwrap().get_gray(8, 8, false), expected, "{scope}");
    }
}

fn first_local_probe() -> (Package, materialcraft_core::NodeId) {
    let mut graph = Graph::new("First quadrant read");
    graph.size = [4; 2];
    graph.format = OutputFormat::F32;
    let stamp = graph.add_node("fx_map", [0.0; 2]);
    let node = graph.node_mut(stamp).unwrap();
    node.params.insert("color_mode".into(), Value::Bool(false));
    node.params.insert("roughness".into(), Value::Float(0.0));
    let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let fields = &mut pattern.nodes[0].parameters;
    fields.insert("patterntype".into(), Value::Int(2));
    let mut tint = FunctionGraph::input("local", ValueType::Float);
    let read = tint.output.unwrap();
    let equal = tint.add("eq", &[read, read], None);
    let yes = tint.add_constant(Value::Float(0.375));
    let no = tint.add_constant(Value::Float(0.625));
    let choose = tint.add("ifelse", &[equal, yes, no], None);
    tint.output = Some(tint.add("set", &[choose], Some(Value::Text("local".into()))));
    fields.insert("opacity".into(), Value::Function(Box::new(tint)));
    node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));

    let mut package = Package::new("Quadrant parameter origin");
    package.graphs.push(graph);
    (package, stamp)
}

#[test]
fn bound_pattern_property_changes_the_first_local_read_at_equal_opacity() {
    let (mut package, stamp) = first_local_probe();
    let mut options = Options::new(&|_| None);
    options.overrides.insert("gain".into(), Value::Float(1.0));
    let cache = Mutex::new(Cache::default());
    for direct in [true, false] {
        let node = package.graphs[0].node_mut(stamp).unwrap();
        if direct {
            node.bindings.insert("opacity".into(), "gain".into());
        } else {
            node.bindings.clear();
            node.functions.insert("opacity".into(), FunctionGraph::input("gain", ValueType::Float));
        }
        for fixed in [true, false, true] {
            options.literal_inputs = if fixed { vec!["gain".into()] } else { Vec::new() };
            let result = cook(&package, 0, &options, &cache);
            assert_eq!(result.nodes[&stamp].error, None);
            let expected = if fixed { 0.375 } else { 0.625 };
            assert_eq!(result.image(stamp, 0).unwrap().get_gray(8, 8, false), expected, "direct {direct}, fixed {fixed}");
        }
    }

    let graph = &mut package.graphs[0];
    graph.node_mut(stamp).unwrap().functions.clear();
    graph.output_size_function = Some(FunctionGraph::input("offset", ValueType::Integer2));
    options.overrides.insert("offset".into(), Value::Int2([0; 2]));
    for (mode, expected) in [(materialcraft_core::SizeMode::RelativeToParent, 0.625), (materialcraft_core::SizeMode::Absolute, 0.375)] {
        let base = &mut package.graphs[0].node_mut(stamp).unwrap().base;
        base.size_mode = mode;
        base.size = if mode == materialcraft_core::SizeMode::Absolute { [4; 2] } else { [0; 2] };
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&stamp].error, None);
        assert_eq!(result.image(stamp, 0).unwrap().get_gray(0, 0, false), expected, "{mode:?}");
    }
}

#[test]
fn connected_image_properties_reach_writable_quadrants() {
    let (mut package, stamp) = first_local_probe();
    let graph = &mut package.graphs[0];
    let uniform = graph.add_node("uniform", [0.0; 2]);
    let source = graph.node_mut(uniform).unwrap();
    source.params.insert("color_mode".into(), Value::Bool(false));
    source.functions.insert("color".into(), FunctionGraph::input("swatch", ValueType::Float4));
    let blur = graph.add_node("blur", [0.0; 2]);
    graph.node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(0.0));
    graph.connect(uniform, "output", blur, "input").unwrap();
    let drawing = graph.add_node("fx_map", [0.0; 2]);
    let mut square = PatternGraph::with_root(PatternNodeKind::Quadrant);
    square.nodes[0].parameters.insert("patterntype".into(), Value::Int(2));
    square.nodes[0].parameters.insert("opacity".into(), Value::Function(Box::new(FunctionGraph::input("swatch", ValueType::Float4))));
    let stage = graph.node_mut(drawing).unwrap();
    stage.params.insert("color_mode".into(), Value::Bool(false));
    stage.params.insert("pattern".into(), Value::PatternGraph(Box::new(square)));
    let mut options = Options::new(&|_| None);
    options.overrides.insert("swatch".into(), Value::Float4([1.0; 4]));
    let cache = Mutex::new(Cache::default());
    for port in ["pattern0", "background"] {
        for source in [uniform, blur, drawing] {
            for (connected, fixed) in [(false, false), (true, true), (true, false), (true, true)] {
                let graph = &mut package.graphs[0];
                graph.disconnect(stamp, port);
                if connected {
                    graph.connect(source, "output", stamp, port).unwrap();
                }
                options.literal_inputs = if fixed { vec!["swatch".into()] } else { Vec::new() };
                let result = cook(&package, 0, &options, &cache);
                assert_eq!(result.nodes[&stamp].error, None);
                let background = if connected && port == "background" { 1.0 } else { 0.0 };
                let first = if connected && !fixed { 0.625 } else { 0.375 };
                assert_eq!(result.image(stamp, 0).unwrap().get_gray(8, 8, false), background + first, "{port}, {source:?}, {connected}, {fixed}");
            }
        }
        package.graphs[0].disconnect(stamp, port);
    }
}

#[test]
fn filter_controls_and_pixel_programs_reach_a_writable_pattern() {
    let (mut package, stamp) = first_local_probe();
    let mut options = Options::new(&|_| None);
    options.only = Some(vec![stamp]);
    let cache = Mutex::new(Cache::default());
    for (filter, property, input) in [
        ("transformation", "offset", Value::Float2([0.0; 2])),
        ("grayscale_conversion", "weights", Value::Float4([1.0, 0.0, 0.0, 0.0])),
        ("directional_blur", "angle", Value::Float(0.25)),
        ("levels", "in_low", Value::Float4([0.0; 4])),
        ("warp", "intensity", Value::Float(0.0)),
        ("pixel_processor", "function", Value::Float(0.5)),
    ] {
        let graph = &mut package.graphs[0];
        let source = graph.add_node(filter, [0.0; 2]);
        let node = graph.node_mut(source).unwrap();
        let program = FunctionGraph::input("driver", ValueType::of(&input).unwrap());
        if filter == "pixel_processor" {
            node.params.insert("color_mode".into(), Value::Bool(false));
            node.params.insert(property.into(), Value::Function(Box::new(program)));
        } else {
            node.functions.insert(property.into(), program);
        }
        graph.disconnect(stamp, "pattern0");
        graph.connect(source, "output", stamp, "pattern0").unwrap();
        options.overrides.insert("driver".into(), input);
        for (fixed, expected) in [(true, 0.375), (false, 0.625), (true, 0.375)] {
            options.literal_inputs = if fixed { vec!["driver".into()] } else { Vec::new() };
            let result = cook(&package, 0, &options, &cache);
            assert_eq!(result.nodes[&source].error, None, "{filter}");
            assert_eq!(result.image(stamp, 0).unwrap().get_gray(8, 8, false), expected, "{filter}, fixed {fixed}");
        }
    }
}

#[test]
fn transform_matte_and_mip_level_follow_the_resolved_sampling_modes() {
    let (mut package, stamp) = first_local_probe();
    let graph = &mut package.graphs[0];
    let transform = graph.add_node("transformation", [0.0; 2]);
    graph.connect(transform, "output", stamp, "pattern0").unwrap();
    let mut options = Options::new(&|_| None);
    options.overrides.insert("ink".into(), Value::Float4([0.0; 4]));
    options.overrides.insert("level".into(), Value::Int(0));
    let cache = Mutex::new(Cache::default());
    for (property, driver) in [("matte", "ink"), ("mip_level", "level")] {
        let node = package.graphs[0].node_mut(transform).unwrap();
        node.bindings.clear();
        node.bindings.insert(property.into(), driver.into());
        node.functions.insert("mip_mode".into(), FunctionGraph::input("mode", ValueType::Integer));
        for axes in [[true, true], [false, true], [true, false], [false, false]] {
            let base = &mut package.graphs[0].node_mut(transform).unwrap().base;
            base.tiling_mode = materialcraft_core::ParameterInheritance::Absolute;
            base.tiling = axes;
            for mode in [0, 1, 0] {
                options.overrides.insert("mode".into(), Value::Int(mode));
                let active = if property == "matte" { axes != [true; 2] } else { mode == 1 };
                let cooked = cook(&package, 0, &options, &cache);
                assert_eq!(cooked.nodes[&transform].error, None);
                assert_eq!(cooked.image(stamp, 0).unwrap().get_gray(8, 8, false), if active { 0.625 } else { 0.375 }, "{property}, {axes:?}, {mode}");
            }
        }
    }
}

#[test]
fn gray_pattern_opacity_keeps_signed_and_undefined_contributions_until_blending() {
    let mut graph = Graph::new("Signed stamp opacity");
    graph.size = [4; 2];
    graph.format = OutputFormat::F32;
    let stamp = graph.add_node("fx_map", [0.0; 2]);
    let node = graph.node_mut(stamp).unwrap();
    node.params.insert("color_mode".into(), Value::Bool(false));
    node.params.insert("roughness".into(), Value::Float(0.0));
    let mut opacity = FunctionGraph::input("%gain", ValueType::Float);
    let numerator = opacity.output.unwrap();
    let denominator = opacity.add_constant(Value::Float(0.0));
    opacity.output = Some(opacity.add("div", &[numerator, denominator], None));
    node.functions.insert("opacity".into(), opacity);

    let mut package = Package::new("Stamp storage");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    let mut options = Options::new(&|_| None);
    let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let fields = &mut pattern.nodes[0].parameters;
    fields.insert("patterntype".into(), Value::Int(2));
    fields.insert("patternsize".into(), Value::Float2([0.5; 2]));
    fields.insert("opacity".into(), Value::Float4([0.25, 0.25, 0.25, 1.0]));
    for (mode, expected) in [(0, [f32::MAX.to_bits(), f32::MIN.to_bits(), 0x7fff_ffff]), (1, [f32::MAX.to_bits(), 0, 0])] {
        pattern.nodes[0].parameters.insert("blendingmode".into(), Value::Int(mode));
        package.graphs[0].node_mut(stamp).unwrap().params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern.clone())));
        for (gain, bits) in [1.0, -1.0, 0.0].into_iter().zip(expected) {
            options.overrides.insert("gain".into(), Value::Float(gain));
            let result = cook(&package, 0, &options, &cache);
            assert_eq!(result.nodes[&stamp].error, None);
            let image = result.image(stamp, 0).unwrap();
            assert_eq!(image.get_gray(8, 8, false).to_bits(), bits);
            assert_eq!(image.get_gray(0, 0, false).to_bits(), 0);
        }
    }

    let child = pattern.add(PatternNodeKind::Quadrant).unwrap();
    pattern.nodes[0].children[0] = Some(child);
    pattern.nodes[0].parameters.insert("blendingmode".into(), Value::Int(0));
    pattern.nodes[0].parameters.insert("patternsize".into(), Value::Float2([1.0; 2]));
    pattern.nodes[1].parameters = pattern.nodes[0].parameters.clone();
    pattern.nodes[1].parameters.insert("opacity".into(), Value::Float4([-0.25, -0.25, -0.25, 1.0]));
    package.graphs[0].node_mut(stamp).unwrap().params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
    options.overrides.insert("gain".into(), Value::Float(1.0));
    let result = cook(&package, 0, &options, &cache);
    let image = result.image(stamp, 0).unwrap();
    assert_eq!(image.get_gray(4, 4, false), f32::MIN);
    assert_eq!(image.get_gray(12, 12, false), f32::MAX);
}
