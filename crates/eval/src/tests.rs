mod font_resource;

use crate::{cache_key::graph_tree_hash, graph_instance::instances_reach};
use craft_texture::{Image, Kind, Limits};
use materialcraft_core::{NodeId, Package, Value};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::*;
use materialcraft_core::Graph;

fn no_res(_: &str) -> Option<Image> {
    None
}

fn sample() -> (Package, NodeId, NodeId, NodeId) {
    let mut g = Graph::new("g");
    g.size = [6, 6];
    let noise = g.add_node("uniform", [0.0, 0.0]);
    let blur = g.add_node("blur", [150.0, 0.0]);
    let out = g.add_node("output", [300.0, 0.0]);
    g.connect(noise, "output", blur, "input").unwrap();
    g.connect(blur, "output", out, "input").unwrap();
    let mut p = Package::new("p");
    p.graphs.push(g);
    (p, noise, blur, out)
}

#[test]
fn cooks_and_caches() {
    let (mut p, noise, blur, out) = sample();
    let cache = Mutex::new(Cache::default());
    let opts = Options::new(&no_res);
    let a = cook(&p, 0, &opts, &cache);

    assert!(a.nodes.values().all(|r| r.error.is_none() && !r.cached));
    assert_eq!(a.image(out, 0).map(|i| (i.width(), i.height())), Some((64, 64)));

    // Same graph: everything from the cache
    let b = cook(&p, 0, &opts, &cache);

    assert!(b.nodes.values().all(|r| r.cached));

    // Editing the blur re-cooks the blur and the output, not the noise
    p.graphs[0].node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(5.0));
    let c = cook(&p, 0, &opts, &cache);

    assert!(c.nodes[&noise].cached && !c.nodes[&blur].cached && !c.nodes[&out].cached);
    assert_eq!(outputs(&p.graphs[0], &c).len(), 1);
}

#[test]
fn fixed_filters_preserve_each_tiling_axis_when_resizing_an_input() {
    use materialcraft_core::{OutputFormat, ParameterInheritance, SizeMode};

    for (filter, port) in [("levels", "input"), ("blend", "foreground"), ("warp", "input")] {
        let mut graph = Graph::new("Tiling");
        let input = graph.add_node("input_grayscale", [0.0; 2]);
        let output = graph.add_node(filter, [100.0, 0.0]);
        for (id, exponent) in [(input, 2), (output, 3)] {
            let base = &mut graph.node_mut(id).unwrap().base;
            base.size_mode = SizeMode::Absolute;
            base.size = [exponent; 2];
            base.format = OutputFormat::U16;
        }
        graph.connect(input, "output", output, port).unwrap();
        let mut package = Package::new("Tiling");
        package.graphs.push(graph);
        let mut options = Options::new(&no_res);
        options.inputs.insert("input".into(), Image::gray(4, 4, |x, y| ((4 * y + x) * 4096) as f32 / 65535.0));
        let cache = Mutex::new(Cache::default());
        for (tiling, expected) in [([false, false], 0), ([true, false], 3072), ([false, true], 12288), ([true, true], 15360), ([true, false], 3072)] {
            let base = &mut package.graphs[0].node_mut(output).unwrap().base;
            base.tiling_mode = ParameterInheritance::Absolute;
            base.tiling = tiling;
            let result = cook(&package, 0, &options, &cache);
            assert_eq!(result.nodes[&output].error, None);
            let pixel = result.image(output, 0).unwrap().get_gray(0, 0, false);
            assert_eq!((pixel * 65535.0).round() as u32, expected, "{filter} {tiling:?}");
        }
    }
}

#[test]
fn subtexel_size_exponents_survive_allocation_and_cache_reuse() {
    use materialcraft_core::{FunctionGraph, OutputFormat, SizeMode};

    let mut function = FunctionGraph::default();
    let size = function.add("get_float2", &[], Some(Value::Text("$size".into())));
    let log = function.add("get_float2", &[], Some(Value::Text("$sizelog2".into())));
    function.output = Some(function.add("vector4", &[size, log], None));
    let mut graph = Graph::new("Reduction");
    graph.size = [7; 2];
    graph.format = OutputFormat::F32;
    let processor = graph.add_node("pixel_processor", [0.0; 2]);
    let node = graph.node_mut(processor).unwrap();
    node.params.insert("function".into(), Value::Function(Box::new(function)));
    node.params.insert("color_mode".into(), Value::Bool(true));
    node.base.size_mode = SizeMode::RelativeToParent;
    let transform = graph.add_node("transformation", [120.0, 0.0]);
    graph.node_mut(transform).unwrap().base.size = [3, 0];
    graph.connect(processor, "output", transform, "input").unwrap();
    let mut package = Package::new("Reduction");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());

    for (offset, first, log, next_width) in [(-8, -2147483648.0, -1.0, 4), (-9, 1073741824.0, -2.0, 2), (-10, 536870912.0, -3.0, 1)] {
        package.graphs[0].node_mut(processor).unwrap().base.size = [offset, -2];
        let cooked = cook(&package, 0, &Options::new(&no_res), &cache);
        assert!(cooked.nodes.values().all(|result| result.error.is_none() && !result.cached));
        let image = cooked.image(processor, 0).unwrap();
        assert_eq!((image.width(), image.height()), (1, 32));
        assert_eq!(image.get(0, 0, false), [first, 32.0, log, 5.0]);
        assert_eq!(cooked.image(transform, 0).unwrap().width(), next_width);
    }
}

#[test]
fn value_processor_stores_integer_lanes_at_single_precision() {
    use materialcraft_core::{FunctionGraph, ValueType};
    let cache = Mutex::new(Cache::default());
    let mut graph = Graph::new("Integer output");
    let processor = graph.add_node("value_processor", [0.0; 2]);
    let mut package = Package::new("Integer output");
    package.graphs.push(graph);
    for (kind, input, expected) in [
        (ValueType::Integer, Value::Int(i32::MAX), Value::Int(i32::MIN)),
        (ValueType::Integer2, Value::Int2([16_777_217, -16_777_219]), Value::Int2([16_777_216, -16_777_220])),
        (ValueType::Integer3, Value::Int3([i32::MIN + 1, 2_147_483_583, 2_147_483_584]), Value::Int3([i32::MIN, 2_147_483_520, i32::MIN])),
        (ValueType::Integer4, Value::Int4([-31, 0, 1, i32::MAX]), Value::Int4([-31, 0, 1, i32::MIN])),
    ] {
        for function in [FunctionGraph::input("numbers", kind), FunctionGraph::constant(input.clone())] {
            package.graphs[0].node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
            let mut options = Options::new(&no_res);
            options.overrides.insert("numbers".into(), input.clone());
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.value(processor, 0), Some(&expected));
            let cached = cook(&package, 0, &options, &cache);
            assert!(cached.nodes[&processor].cached);
            assert_eq!(cached.value(processor, 0), Some(&expected));
        }
    }
}

#[test]
fn material_overrides_skip_bound_preview_decodes_and_release_buffers() {
    let mut graph = Graph::new("Bound inputs");
    graph.size = [4; 2];
    let identifiers = ["height", "curvature", "thickness"];
    let mut outputs = Vec::new();
    for identifier in identifiers {
        let input = graph.add_node("input_grayscale", [0.0; 2]);
        graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text(identifier.into()));
        let output = graph.add_node("output", [0.0; 2]);
        graph.connect(input, "output", output, "input").unwrap();
        graph.input_images.insert(identifier.into(), format!("preview_{identifier}"));
        outputs.push(output);
    }
    let mut package = Package::new("Bound inputs");
    package.graphs.push(graph);
    let preview_buffers = Mutex::new(Vec::new());
    let read_preview = |_: &str| {
        let image = Image::filled(1024, 1024, Kind::Gray, [0.25; 4]);
        preview_buffers.lock().unwrap().push(std::sync::Arc::downgrade(image.shared_data()));
        Some(image)
    };
    let cache = Mutex::new(Cache::new(0));

    for value in [0.125, 0.5, 0.875] {
        let mut options = Options::new(&read_preview);
        let mut supplied_buffers = Vec::new();
        for identifier in &identifiers[..2] {
            let image = Image::filled(16, 16, Kind::Gray, [value; 4]);
            supplied_buffers.push(std::sync::Arc::downgrade(image.shared_data()));
            options.inputs.insert((*identifier).into(), image);
        }

        let result = cook(&package, 0, &options, &cache);
        for (output, expected) in outputs.iter().zip([value, value, 0.25]) {
            let image = result.image(*output, 0).unwrap();
            assert!((image.mean()[0] - expected).abs() < 0.005);
        }
        drop(result);
        drop(options);

        assert!(supplied_buffers.iter().all(|buffer| buffer.upgrade().is_none()));
    }

    let previews = preview_buffers.into_inner().unwrap();
    assert!(previews.iter().all(|buffer| buffer.upgrade().is_none()));
    eprintln!("Decoded preview planes: {} ({} bytes of grayscale payload)", previews.len(), previews.len() * 1024 * 1024 * size_of::<f32>());
    assert_eq!(previews.len(), 3, "one fallback preview decode per cook");
}

#[test]
fn selected_outputs_decode_only_reachable_input_previews() {
    let mut graph = Graph::new("Preview selection");
    graph.size = [4; 2];
    let mut outputs = Vec::new();
    for identifier in ["height", "roughness", "normal", "height"] {
        let input = graph.add_node("input_grayscale", [0.0; 2]);
        graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text(identifier.into()));
        graph.input_images.insert(identifier.into(), identifier.into());
        let output = graph.add_node("output", [0.0; 2]);
        graph.connect(input, "output", output, "input").unwrap();
        outputs.push(output);
    }

    let mut package = Package::new("Preview selection");
    package.graphs.push(graph);
    let mut counts = Vec::new();
    for (selected, expected) in [(vec![outputs[0], outputs[3]], 0.25), (vec![outputs[1]], 0.5), (vec![outputs[2]], 0.75)] {
        let buffers = Mutex::new(Vec::new());
        let peak_planes = std::sync::atomic::AtomicUsize::new(0);
        let read_preview = |name: &str| {
            let value = match craft_texture::split_resource_key(name).0 {
                "height" => 0.25,
                "roughness" => 0.5,
                "normal" => 0.75,
                other => panic!("unexpected preview {other}"),
            };
            let image = Image::filled(1024, 1024, Kind::Gray, [value; 4]);
            let mut buffers = buffers.lock().unwrap();
            buffers.push(std::sync::Arc::downgrade(image.shared_data()));
            peak_planes.fetch_max(buffers.iter().filter(|plane| plane.strong_count() > 0).count(), std::sync::atomic::Ordering::Relaxed);
            Some(image)
        };
        let mut options = Options::new(&read_preview);
        options.only = Some(selected.clone());
        let cache = Mutex::new(Cache::new(0));

        let result = cook(&package, 0, &options, &cache);

        for output in &outputs {
            if selected.contains(output) {
                assert!((result.image(*output, 0).unwrap().mean()[0] - expected).abs() < 0.005);
            } else {
                assert!(result.image(*output, 0).is_none());
            }
        }
        let buffers = buffers.into_inner().unwrap();
        assert!(buffers.iter().all(|buffer| buffer.upgrade().is_none()));
        counts.push((buffers.len(), peak_planes.into_inner()));
    }

    eprintln!("Decoded and peak live 1024-square grayscale previews per selection: {counts:?}");
    assert_eq!(counts, [(1, 1); 3]);
}

#[test]
fn processor_tiling_variable_tracks_both_axes_and_invalidates_cached_values() {
    let mut function = materialcraft_core::FunctionGraph::default();
    function.output = Some(function.add("get_integer1", &[], Some(Value::Text("$tiling".into()))));
    let mut graph = Graph::new("Tiling");
    let processor = graph.add_node("value_processor", [0.0; 2]);
    graph.node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    let mut package = Package::new("Tiling");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());

    for (tiling, expected) in [([false, false], 0), ([true, false], 1), ([false, true], 2), ([true, true], 3)] {
        package.graphs[0].node_mut(processor).unwrap().base.tiling = tiling;
        package.graphs[0].node_mut(processor).unwrap().base.tiling_mode = materialcraft_core::ParameterInheritance::Absolute;
        let result = cook(&package, 0, &Options::new(&no_res), &cache);

        assert_eq!(result.nodes[&processor].error, None);
        assert_eq!(result.nodes[&processor].outputs[0].as_value(), Some(&Value::Int(expected)));
        assert!(!result.nodes[&processor].cached);
        let cached = cook(&package, 0, &Options::new(&no_res), &cache);
        assert!(cached.nodes[&processor].cached);
        assert_eq!(cached.nodes[&processor].outputs[0].as_value(), Some(&Value::Int(expected)));
    }
}

#[test]
fn base_parameter_assignments_reach_pixel_functions_without_leaking_to_other_nodes() {
    use materialcraft_core::{BaseParameter, FunctionGraph};

    let mut initialization = FunctionGraph::default();
    let value = initialization.add_constant(Value::Float(0.375));
    let assignment = initialization.add("set", &[value], Some(Value::Text("prepared_value".into())));
    let output_size = initialization.add_constant(Value::Int2([0; 2]));
    initialization.output = Some(initialization.add("sequence", &[assignment, output_size], None));
    let mut function = FunctionGraph::default();
    function.output = Some(function.add("get_float1", &[], Some(Value::Text("prepared_value".into()))));

    let mut graph = Graph::new("Parameter scope");
    graph.size = [4; 2];
    graph.format = materialcraft_core::OutputFormat::F32;
    let initialized = graph.add_node("pixel_processor", [0.0; 2]);
    let isolated = graph.add_node("pixel_processor", [100.0; 2]);
    for id in [initialized, isolated] {
        graph.node_mut(id).unwrap().params.insert("function".into(), Value::Function(Box::new(function.clone())));
        graph.node_mut(id).unwrap().params.insert("color_mode".into(), Value::Bool(false));
    }
    graph.node_mut(initialized).unwrap().base.functions.insert(BaseParameter::OutputSize, initialization);
    let mut package = Package::new("Parameter scope");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    for expected in [0.375, 0.625] {
        package.graphs[0].node_mut(initialized).unwrap().base.functions.get_mut(&BaseParameter::OutputSize).unwrap().nodes[0].value =
            Some(Value::Float(expected));

        let cooked = cook(&package, 0, &Options::new(&no_res), &cache);

        assert_eq!(cooked.nodes[&initialized].error, None);
        assert!(!cooked.nodes[&initialized].cached);
        assert!((cooked.image(initialized, 0).unwrap().mean()[0] - expected).abs() < 1e-6);

        // Unassigned user variables start at typed zero in each node's own scope
        assert_eq!(cooked.nodes[&isolated].error, None);
        assert_eq!(cooked.image(isolated, 0).unwrap().mean()[0], 0.0);
    }
}

#[test]
fn hostile_param_is_clamped_before_cooking() {
    let (mut p, noise, _, _) = sample();
    let blur = p.graphs[0].add_node("directional_blur", [150.0, 100.0]);
    p.graphs[0].connect(noise, "output", blur, "input").unwrap();
    p.graphs[0].node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(1e30));
    let cache = Mutex::new(Cache::default());
    let c = cook(&p, 0, &Options::new(&no_res), &cache);

    assert!(c.nodes[&blur].error.is_none(), "{:?}", c.nodes[&blur].error);
    assert_eq!(c.image(blur, 0).map(|i| (i.width(), i.height())), Some((64, 64)));
}

#[test]
fn preview_cap_and_unknown_nodes() {
    let (mut p, _, _, out) = sample();
    p.graphs[0].size = [10, 10];
    let bad = p.graphs[0].add_node("no_such_node", [0.0, 0.0]);
    let cache = Mutex::new(Cache::default());
    let mut opts = Options::new(&no_res);
    opts.max_size = Some(128);
    let c = cook(&p, 0, &opts, &cache);

    assert_eq!(c.image(out, 0).map(|i| i.width()), Some(128));
    assert!(c.nodes[&bad].error.is_some());
}

#[test]
fn typed_instance_ports_preserve_explicit_arguments_and_wire_fallbacks() {
    use materialcraft_core::{FunctionGraph, ValueType};

    let (mut parent, instance) = instancing("Caller", "Echo");
    let wire = parent.add_node("input_value", [0.0; 2]);
    parent.node_mut(wire).unwrap().params.insert("default".into(), Value::Int(9));
    let mut child = Graph::new("Echo");
    let bridge = child.add_node("input_value", [0.0; 2]);
    child.node_mut(bridge).unwrap().params.extend([("identifier".into(), Value::Text("count".into())), ("default".into(), Value::Int(5))]);
    let output = child.add_node("output", [0.0; 2]);
    child.connect(bridge, "output", output, "input").unwrap();

    let mut package = Package::new("Typed arguments");
    package.graphs.extend([parent, child]);
    let cache = Mutex::new(Cache::default());
    let mut options = Options::new(&no_res);
    options.overrides.insert("control".into(), Value::Int(-12));
    for wired in [false, true] {
        if wired {
            package.graphs[0].connect(wire, "output", instance, "count").unwrap();
        }
        for (literal, function, expected) in [(None, false, if wired { 9 } else { 5 }), (Some(0), false, 0), (Some(-3), false, -3), (None, true, -12)] {
            let node = package.graphs[0].node_mut(instance).unwrap();
            node.params.remove("count");
            node.functions.remove("count");
            if let Some(value) = literal {
                node.params.insert("count".into(), Value::Int(value));
            }
            if function {
                node.functions.insert("count".into(), FunctionGraph::input("control", ValueType::Integer));
            }
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&instance].error, None);
            assert_eq!(cooked.nodes[&instance].outputs[0].as_value(), Some(&Value::Int(expected)), "wire {wired}, stored {literal:?}, function {function}");
        }
    }
}

#[test]
fn cached_instances_distinguish_fixed_and_bound_integer_arguments() {
    use materialcraft_core::{ExposedParameter, FunctionGraph, ValueType};
    let (parent, instance) = instancing("Parent", "Selector");
    let mut child = Graph::new("Selector");
    child.exposed.push(ExposedParameter {
        connectable: false,
        id: "index".into(),
        label: "Index".into(),
        default: Value::Int(0),
        description: String::new(),
        group: String::new(),
        editor: None,
        clamp: false,
        step: None,
        min: -8.0,
        max: 8.0,
        options: Vec::new(),
    });
    let mut function = FunctionGraph::input("%index", ValueType::Integer);
    let input = function.output.unwrap();
    function.output = Some(function.add("tofloat", &[input], None));
    let processor = child.add_node("value_processor", [0.0; 2]);
    child.node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    let output = child.add_node("output", [0.0; 2]);
    child.connect(processor, "output", output, "input").unwrap();
    let mut package = Package::new("Integer arguments");
    package.graphs.extend([parent, child]);
    let mut options = Options::new(&no_res);
    options.overrides.insert("driver".into(), Value::Int(-5));
    let cache = Mutex::new(Cache::default());
    for bound in [false, true, false] {
        let node = package.graphs[0].node_mut(instance).unwrap();
        node.params.insert("index".into(), Value::Int(-5));
        if bound {
            node.bindings.insert("index".into(), "driver".into());
        } else {
            node.bindings.remove("index");
        }
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&instance].error, None);
        assert_eq!(result.nodes[&instance].outputs[0].as_value(), Some(&Value::Float(if bound { -5.0 } else { 4_294_967_296.0 })));
    }
}

#[test]
fn instanced_float_angles_keep_their_precision_across_an_unrelated_write() {
    assert_instance_angle_precision(Value::Float(1.0));
}

#[test]
fn instanced_vector_angles_preserve_fixed_and_parent_bound_precision() {
    for turn in [Value::Float2([1.0, 0.25]), Value::Float3([1.0, 0.25, -0.5]), Value::Float4([1.0, 0.25, -0.5, 0.75])] {
        assert_instance_angle_precision(turn);
    }
}

fn assert_instance_angle_precision(argument: Value) {
    use materialcraft_core::{ExposedParameter, FunctionGraph, GraphInstance, GraphPort, OutputFormat, ValueType};
    let (parent, instance) = instancing("Parent", "Angle");
    let mut child = Graph::new("Angle");
    child.exposed.push(ExposedParameter {
        id: "turn".into(),
        label: "Turn".into(),
        default: argument.clone(),
        connectable: false,
        clamp: false,
        description: String::new(),
        group: String::new(),
        editor: None,
        step: None,
        min: -2.0,
        max: 2.0,
        options: Vec::new(),
    });
    let mut body = FunctionGraph::default();
    let one = body.add_constant(Value::Float(1.0));
    body.output = Some(body.add("set", &[one], Some(Value::Text("scratch".into()))));
    let mut expression = FunctionGraph::input("turn", ValueType::of(&argument).unwrap());
    let input = expression.output.unwrap();
    let write = expression.add("instance", &[], None);
    let call = expression.nodes.last_mut().unwrap();
    call.instance = Some(GraphInstance {
        label: String::new(),
        graph: "Remember".into(),
        package: String::new(),
        inputs: Vec::new(),
        outputs: vec![GraphPort { identifier: "output".into(), type_code: 256 }],
        defaults: Default::default(),
    });
    call.source = Some(Ok(std::sync::Arc::new(body)));
    let turn = if matches!(argument, Value::Float(_)) { input } else { expression.add("swizzle1", &[input], Some(Value::Int(0))) };
    let tau = expression.add_constant(Value::Float(std::f32::consts::TAU));
    let angle = expression.add("mul", &[turn, tau], None);
    let sine = expression.add("sin", &[angle], None);
    expression.output = Some(expression.add("sequence", &[write, sine], None));
    let processor = child.add_node("pixel_processor", [0.0; 2]);
    let pixel = child.node_mut(processor).unwrap();
    pixel.base.format = OutputFormat::F32;
    pixel.params.insert("function".into(), Value::Function(Box::new(expression)));
    let output = child.add_node("output", [0.0; 2]);
    child.connect(processor, "output", output, "input").unwrap();
    let mut package = Package::new("Fixed and bound turns");
    package.graphs.extend([parent, child]);
    let mut options = Options::new(&no_res);
    options.overrides.insert("driver".into(), argument.clone());
    let cache = Mutex::new(Cache::default());
    for (explicit, bound) in [(false, false), (true, false), (true, true), (true, false)] {
        let node = package.graphs[0].node_mut(instance).unwrap();
        if explicit {
            node.params.insert("turn".into(), argument.clone());
        }
        if bound {
            node.bindings.insert("turn".into(), "driver".into());
        } else {
            node.bindings.remove("turn");
        }
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&instance].error, None);
        let red = result.image(instance, 0).unwrap().get(0, 0, false)[0];
        assert_eq!(red.to_bits(), if bound { (-0.0_f32).to_bits() } else { 0x343b_bd2e }, "{argument:?}, explicit {explicit}, bound {bound}");
    }
}

#[test]
fn graph_instances_cook_their_subgraph() {
    let (mut p, ..) = sample();

    // A second graph: input → invert → output, instanced from the first
    let mut sub = Graph::new("inv");
    sub.size = [6, 6];
    let i = sub.add_node("input_grayscale", [0.0, 0.0]);
    let inv = sub.add_node("levels", [100.0, 0.0]);
    sub.node_mut(inv).unwrap().params.insert("out_low".into(), Value::Float4([1.0; 4]));
    sub.node_mut(inv).unwrap().params.insert("out_high".into(), Value::Float4([0.0; 4]));
    let o = sub.add_node("output", [200.0, 0.0]);
    sub.connect(i, "output", inv, "input").unwrap();
    sub.connect(inv, "output", o, "input").unwrap();
    p.graphs.push(sub);
    let g = &mut p.graphs[0];
    let u = g.add_node("uniform", [0.0, 100.0]);
    g.node_mut(u).unwrap().params.insert("color_mode".into(), Value::Bool(false));
    g.node_mut(u).unwrap().params.insert("color".into(), Value::Color([0.25, 0.25, 0.25, 1.0]));
    let inst = g.add_node("graph_instance", [100.0, 100.0]);
    g.node_mut(inst).unwrap().params.insert("graph".into(), Value::Text("inv".into()));
    g.connect(u, "output", inst, "input").unwrap();

    // A graph instancing itself is refused, not a stack overflow
    let selfi = g.add_node("graph_instance", [0.0, 200.0]);
    g.node_mut(selfi).unwrap().params.insert("graph".into(), Value::Text("g".into()));
    let cache = Mutex::new(Cache::default());
    let c = cook(&p, 0, &Options::new(&no_res), &cache);
    let img = c.image(inst, 0).unwrap();

    assert!((img.data()[0] - (1.0 - 0.0722 * 0.25 - 0.2126 * 0.25 - 0.7152 * 0.25)).abs() < 1e-4, "{}", img.data()[0]);
    assert!(c.nodes[&selfi].error.as_deref().is_some_and(|e| e.contains("uses itself")), "{:?}", c.nodes[&selfi].error);
}

/// A graph `name` holding one instance of `target` wired to an output
fn instancing(name: &str, target: &str) -> (Graph, NodeId) {
    let mut g = Graph::new(name);
    g.size = [4, 4];
    let inst = g.add_node("graph_instance", [0.0, 0.0]);
    g.node_mut(inst).unwrap().params.insert("graph".into(), Value::Text(target.into()));
    let o = g.add_node("output", [100.0, 0.0]);
    g.connect(inst, "output", o, "input").unwrap();
    (g, inst)
}

#[test]
fn an_instanced_graph_uses_its_declared_absolute_output_size() {
    let (parent, instance) = instancing("Parent", "Rectangle");
    let mut child = Graph::new("Rectangle");
    child.absolute_output_size = Some([6, 5]);
    let generator = child.add_node("uniform", [0.0; 2]);
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(generator, "output", output, "input").unwrap();
    let mut package = Package::new("Dimensions");
    package.graphs.extend([parent, child]);
    let cache = Mutex::new(Cache::default());
    for (size, expected) in [(Some([6, 5]), [64, 32]), (Some([5, 0]), [32, 1]), (None, [16, 16])] {
        package.graphs[1].absolute_output_size = size;
        let cooked = cook(&package, 0, &Options::new(&no_res), &cache);
        let image = cooked.image(instance, 0).unwrap();
        assert_eq!([image.width(), image.height()], expected);
        assert_eq!(cooked.nodes[&instance].base.size, expected);
    }
}

#[test]
fn instance_outputs_keep_each_sources_dimensions_and_precision() {
    use materialcraft_core::{OutputFormat, SizeMode};
    let (mut parent, instance) = instancing("Parent", "TwoOutputs");
    let mut source = Graph::new("TwoOutputs");
    let mut consumers = Vec::new();
    for (name, size, format) in [("output", [6, 5], OutputFormat::F32), ("small", [2, 3], OutputFormat::U8)] {
        let fill = source.add_node("uniform", [0.0; 2]);
        let base = &mut source.node_mut(fill).unwrap().base;
        base.size_mode = SizeMode::Absolute;
        base.size = size;
        base.format = format;
        let bridge = source.add_node("output", [100.0, 0.0]);
        source.node_mut(bridge).unwrap().params.insert("identifier".into(), Value::Text(name.into()));
        source.connect(fill, "output", bridge, "input").unwrap();
        let consumer = parent.add_node("levels", [200.0, 0.0]);
        parent.connect(instance, name, consumer, "input").unwrap();
        consumers.push((consumer, size.map(|power| 1u32 << power), format));
    }
    let mut package = Package::new("Per-port dimensions");
    package.graphs.extend([parent, source]);
    let cache = Mutex::new(Cache::default());
    for cached in [false, true] {
        let cooked = cook(&package, 0, &Options::new(&no_res), &cache);
        assert_eq!(cooked.nodes[&instance].cached, cached);
        for &(id, size, format) in &consumers {
            let result = &cooked.nodes[&id];
            assert_eq!(result.base.size, size);
            assert_eq!(result.base.format, format);
            let image = cooked.image(id, 0).unwrap();
            assert_eq!([image.width(), image.height()], size);
        }
    }
}

#[test]
fn editing_a_nested_graph_recooks_the_top() {
    let mut c = Graph::new("c");
    c.size = [4, 4];
    let u = c.add_node("uniform", [0.0, 0.0]);
    c.node_mut(u).unwrap().params.insert("color_mode".into(), Value::Bool(false));
    c.node_mut(u).unwrap().params.insert("color".into(), Value::Color([0.2, 0.2, 0.2, 1.0]));
    let o = c.add_node("output", [100.0, 0.0]);
    c.connect(u, "output", o, "input").unwrap();
    let (b, _) = instancing("b", "c");
    let (a, top) = instancing("a", "b");
    let mut p = Package::new("p");
    p.graphs = vec![a, b, c];
    let cache = Mutex::new(Cache::default());
    let opts = Options::new(&no_res);
    let first = cook(&p, 0, &opts, &cache);
    let out_a = p.graphs[0].nodes.iter().find(|n| n.def == "output").unwrap().id;
    assert!((first.image(out_a, 0).unwrap().mean()[0] - 0.2).abs() < 0.01);
    p.graphs[2].node_mut(u).unwrap().params.insert("color".into(), Value::Color([0.8, 0.8, 0.8, 1.0]));
    let second = cook(&p, 0, &opts, &cache);
    assert!(!second.nodes[&top].cached);
    assert!((second.image(out_a, 0).unwrap().mean()[0] - 0.8).abs() < 0.01);
}

#[test]
fn deep_shared_instance_walks_stay_bounded() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut pkg = Package::new("deep");
            for index in 0..2000 {
                let mut graph = Graph::new(&format!("g{index}"));

                // Repeated references must visit the next graph only once
                for _ in 0..4 {
                    let id = graph.add_node("graph_instance", [0.0; 2]);
                    graph.node_mut(id).unwrap().params.insert("graph".into(), Value::Text(format!("g{}", index + 1)));
                }
                pkg.graphs.push(graph);
            }

            assert!(instances_reach(&pkg, 0, 1999));
            assert!(!instances_reach(&pkg, 0, 2000));

            let before = graph_tree_hash(&pkg, 0);
            pkg.graphs[1999].seed = 42;

            assert_ne!(before, graph_tree_hash(&pkg, 0));

            // A back edge must terminate both traversals too
            pkg.graphs[1999].nodes[0].params.insert("graph".into(), Value::Text("g0".into()));

            assert!(instances_reach(&pkg, 1, 0));
            assert!(!instances_reach(&pkg, 0, 2000));
            assert_ne!(before, graph_tree_hash(&pkg, 0));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn mutual_instances_are_an_error() {
    let (x, xi) = instancing("x", "y");
    let (y, _) = instancing("y", "x");
    let mut p = Package::new("p");
    p.graphs = vec![x, y];
    let cache = Mutex::new(Cache::default());
    let c = cook(&p, 0, &Options::new(&no_res), &cache);
    assert!(c.nodes[&xi].error.as_deref().is_some_and(|e| e.contains("uses itself")), "{:?}", c.nodes[&xi].error);
}

#[test]
fn same_size_resources_key_differently() {
    use materialcraft_core::{Resource, ResourceBytes};
    use std::sync::atomic::AtomicU8;
    let mut g = Graph::new("g");
    g.size = [4, 4];
    let bm = g.add_node("bitmap", [0.0, 0.0]);
    g.node_mut(bm).unwrap().params.insert("resource".into(), Value::Text("tex".into()));
    let mut p = Package::new("p");
    p.graphs.push(g);
    p.resources.push(Resource::new("tex", "tex.bin", ResourceBytes::new(vec![1; 100])));

    // Stands in for decoding: the first byte picks the shade
    let shade = AtomicU8::new(1);
    let res = |_: &str| Some(Image::filled(4, 4, Kind::Gray, [f32::from(shade.load(Ordering::Relaxed)) / 10.0; 4]));
    let cache = Mutex::new(Cache::default());
    let opts = Options::new(&res);
    let first = cook(&p, 0, &opts, &cache);
    let mean = first.image(bm, 0).unwrap().mean()[0];
    p.resources[0].bytes = ResourceBytes::new(vec![2; 100]);
    shade.store(2, Ordering::Relaxed);
    let second = cook(&p, 0, &opts, &cache);

    assert!(!second.nodes[&bm].cached);
    assert!((second.image(bm, 0).unwrap().mean()[0] - mean).abs() > 0.05);
}

#[test]
fn box_blur_uses_the_resolved_pixel_size_on_each_axis() {
    use materialcraft_core::{BaseParameter, FunctionGraph, OutputFormat, ParameterInheritance};

    let mut graph = Graph::new("Blur footprint");
    graph.size = [5, 5];
    graph.format = OutputFormat::F32;
    let source = graph.add_node("input_grayscale", [0.0; 2]);
    let blur = graph.add_node("blur", [0.0; 2]);
    let node = graph.node_mut(blur).unwrap();
    node.params.insert("intensity".into(), Value::Float(1.0));
    node.base.pixel_size_mode = ParameterInheritance::Absolute;
    graph.connect(source, "output", blur, "input").unwrap();
    let mut package = Package::new("Blur footprint");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(32, 32, |x, y| f32::from(x == 16 && y == 16)));
    let cache = Mutex::new(Cache::default());
    for (size, expected) in
        [([64.0, 64.0], [1.0 / 256.0, 1.0 / 512.0, 1.0 / 512.0]), ([1.0, 64.0], [1.0 / 16.0, 0.0, 1.0 / 32.0]), ([64.0, 1.0], [1.0 / 16.0, 1.0 / 32.0, 0.0])]
    {
        let base = &mut package.graphs[0].node_mut(blur).unwrap().base;
        base.functions.insert(BaseParameter::PixelSize, FunctionGraph::constant(Value::Float2(size)));
        let result = cook(&package, 0, &options, &cache);
        let image = result.image(blur, 0).unwrap();
        assert_eq!([(16, 16), (8, 16), (16, 8)].map(|(x, y)| image.get_gray(x, y, false)), expected, "pixel size {size:?}");
    }
}

#[test]
fn box_blur_multiplies_signed_pixel_sizes_before_limiting_its_radius() {
    use materialcraft_core::{BaseParameter, OutputFormat, ParameterInheritance};

    let mut graph = Graph::new("Inherited footprint");
    graph.size = [5, 5];
    graph.format = OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let blur = graph.add_node("blur", [0.0; 2]);
    graph.node_mut(input).unwrap().base.pixel_size_mode = ParameterInheritance::Absolute;
    graph.node_mut(blur).unwrap().base.pixel_size_mode = ParameterInheritance::RelativeToInput;
    graph.node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(1.0));
    graph.connect(input, "output", blur, "input").unwrap();
    let mut package = Package::new("Inherited footprint");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(32, 32, |x, y| f32::from(x == 16 && y == 16)));
    let cache = Mutex::new(Cache::default());
    for (parent, factor, expected) in [([-2.0, 3.0], [-64.0, 8.0], 1.0 / 192.0), ([-2.0, 3.0], [64.0, -8.0], 1.0), ([0.0, 4.0], [8.0, -4.0], 1.0)] {
        for (node, size) in [(input, parent), (blur, factor)] {
            BaseParameter::PixelSize.set_value(&mut package.graphs[0].node_mut(node).unwrap().base, &Value::Float2(size)).unwrap();
        }
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.image(blur, 0).unwrap().get_gray(16, 16, false), expected);
    }
}

#[test]
fn emboss_uses_the_resolved_pixel_size_on_each_axis() {
    use materialcraft_core::{OutputFormat, ParameterInheritance};

    let mut graph = Graph::new("Relief");
    graph.size = [5, 5];
    graph.format = OutputFormat::F32;
    let [base, height] = ["base", "height"].map(|name| {
        let id = graph.add_node("input_grayscale", [0.0; 2]);
        graph.node_mut(id).unwrap().params.insert("identifier".into(), Value::Text(name.into()));
        id
    });
    let emboss = graph.add_node("emboss", [0.0; 2]);
    let node = graph.node_mut(emboss).unwrap();
    node.base.pixel_size_mode = ParameterInheritance::Absolute;
    node.base.pixel_size = [0.5, 2.0];
    node.params.insert("intensity".into(), Value::Float(1.0));
    graph.connect(base, "output", emboss, "input").unwrap();
    graph.connect(height, "output", emboss, "height").unwrap();
    let mut package = Package::new("Relief");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("base".into(), Image::filled(32, 32, Kind::Gray, [0.5; 4]));
    options.inputs.insert("height".into(), Image::gray(32, 32, |x, y| if x == 16 && y == 16 { 1.0 } else { 0.0 }));
    let cache = Mutex::new(Cache::default());
    for (angle, east, south, center) in [(0.0, 0.4375, 0.5, 0.5625), (0.25, 0.5, 0.25, 0.75)] {
        package.graphs[0].node_mut(emboss).unwrap().params.insert("light_angle".into(), Value::Float(angle));
        let result = cook(&package, 0, &options, &cache);
        let image = result.image(emboss, 0).unwrap();
        assert_eq!([image.get_gray(17, 16, false), image.get_gray(16, 17, false), image.get_gray(16, 16, false)], [east, south, center]);
    }
}

#[test]
fn bitmap_growth_maps_the_origin_and_half_texel_offset_separately() {
    let mut graph = Graph::new("Column");
    graph.size = [0, 6];
    graph.format = materialcraft_core::OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let mut package = Package::new("Column");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    let column = Image::gray(1, 49, |_, y| {
        f32::from_bits(match y {
            5 => 0xbeae_712e,
            6 => 0x3f90_4fe4,
            _ => 0,
        })
    });
    options.inputs.insert("input".into(), column);
    let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
    assert_eq!(result.image(input, 0).unwrap().get_gray(0, 7, false).to_bits(), 0x3c73_7400);
}

#[test]
fn filters_sample_bound_integer_bitmaps_at_the_source_depth() {
    use OutputFormat::{F16, F32, U8, U16};
    use materialcraft_core::{OutputFormat, ParameterInheritance};

    let mut graph = Graph::new("Source precision");
    graph.size = [5, 5];
    graph.format = F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let transform = graph.add_node("transformation", [0.0; 2]);
    let output = graph.add_node("output", [0.0; 2]);
    let node = graph.node_mut(transform).unwrap();
    node.base.format_mode = ParameterInheritance::Absolute;
    node.base.format = F32;
    node.params.insert("offset".into(), Value::Float2([15.0 / 1024.0, -21.0 / 1024.0]));
    graph.connect(input, "output", transform, "input").unwrap();
    graph.connect(input, "output", output, "input").unwrap();
    let mut package = Package::new("Integer input");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    for (depth, bridge, expected) in [(U8, F16, 0x3eee_4eee), (U8, F32, 0x3eee_4eee), (U16, F32, 0x3eee_92ef), (U16, F16, 0x3eee_8000)] {
        let node = package.graphs[0].node_mut(input).unwrap();
        node.base.format_mode = ParameterInheritance::Absolute;
        node.base.format = bridge;
        let mut options = Options::new(&no_res);
        options.bitmap_formats.insert("input".into(), depth);
        let image = Image::gray(32, 17, |x, y| {
            let level = if x % 2 == 0 { 9 } else { 243 };
            if depth == U8 { level as f32 / 255.0 } else { (level * 257 + (7 * x + 3 * y) % 251) as f32 / 65535.0 }
        });
        options.inputs.insert("input".into(), image.with_integer_sampling(true));
        for _ in 0..2 {
            let result = cook(&package, 0, &options, &cache);
            assert!(result.nodes.values().all(|node| node.error.is_none()));
            assert_eq!(result.image(transform, 0).unwrap().get_gray(0, 0, false).to_bits(), expected);
            if depth == U8 && bridge == F16 {
                for id in [input, output] {
                    assert_eq!(result.image(id, 0).unwrap().get_gray(0, 0, false).to_bits(), 0x3d10_8000);
                }
            }
        }
    }
}

#[test]
fn bitmap_half_storage_uses_the_original_integer_channel_depth() {
    use materialcraft_core::OutputFormat::{F16, U8, U16};

    let mut graph = Graph::new("Byte and word");
    graph.size = [5, 5];
    graph.format = F16;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let mut package = Package::new("Integer source");
    package.graphs.push(graph);
    for (depth, level, expected) in [
        (U8, 9.0 / 255.0, [0x3d10_8000; 3]),
        (U16, 2313.0 / 65535.0, [0x3d10_0000, 0x3d10_2000, 0x3d10_c000]),
        (U16, 1.0, [0x3f7f_e000, 0x3f7f_e000, 0x3f80_0000]),
    ] {
        let mut options = Options::new(&no_res);
        options.bitmap_formats.insert("input".into(), depth);
        options.inputs.insert("input".into(), Image::filled(32, 17, Kind::Gray, [level; 4]).with_integer_sampling(true));
        let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
        let image = result.image(input, 0).unwrap();
        assert_eq!([0, 1, 2].map(|x| image.get_gray(x, 0, false).to_bits()), expected);
    }
}

fn bitmap_impulse_graph() -> (Graph, NodeId, NodeId) {
    use materialcraft_core::{OutputFormat, ParameterInheritance, PatternGraph, PatternNodeKind};

    let mut graph = Graph::new("Bitmap mip levels");
    graph.size = [4, 6];
    graph.format = OutputFormat::U16;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let stamp = graph.add_node("fx_map", [0.0; 2]);
    let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
    pattern.nodes[0].parameters.insert("patterntype".into(), Value::Int(1));
    pattern.nodes[0].parameters.insert("patternsize".into(), Value::Float2([0.25; 2]));
    let node = graph.node_mut(stamp).unwrap();
    node.base.format_mode = ParameterInheritance::Absolute;
    node.base.format = OutputFormat::U16;
    node.params.insert("color_mode".into(), Value::Bool(false));
    node.params.insert("roughness".into(), Value::Float(0.0));
    node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
    graph.connect(input, "output", stamp, "pattern0").unwrap();
    (graph, input, stamp)
}

#[test]
fn pattern_mips_of_a_resized_bitmap_preserve_the_measured_impulse() {
    let (graph, input, stamp) = bitmap_impulse_graph();
    let mut package = Package::new("Impulse");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(32, 16, |x, y| f32::from(x == 10 && y == 8)));
    let cache = Mutex::new(Cache::default());
    for expected in [[0, 2048, 12288, 2048, 0], [0, 1280, 5632, 1280, 0]] {
        let result = cook(&package, 0, &options, &cache);
        assert!(result.nodes.values().all(|node| node.error.is_none()));
        let image = result.image(stamp, 0).unwrap();
        let levels = [30, 31, 32, 33, 34].map(|y| (image.get_gray(7, y, false) * 65535.0).round() as u16);
        assert_eq!(levels, expected);
        // Binding the identical bridge pixels at their final size supplies an ordinary mip chain.
        options.inputs.insert("input".into(), result.image(input, 0).unwrap().clone());
    }
}

#[test]
fn switch_color_storage_depends_on_whether_opacity_reads_a_graph_parameter() {
    use materialcraft_core::{FunctionGraph, OutputFormat, ValueType};

    let mut graph = Graph::new("Endpoint opacity");
    graph.size = [2; 2];
    let blend = graph.add_node("blend", [0.0; 2]);
    graph.node_mut(blend).unwrap().params.insert("mode".into(), Value::Enum(7));
    let mut options = Options::new(&no_res);
    for (port, level) in [("foreground", 0.5), ("background", 0.25)] {
        let input = graph.add_node("input_color", [0.0; 2]);
        graph.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text(port.into()));
        graph.connect(input, "output", blend, port).unwrap();
        options.inputs.insert(
            port.into(),
            Image::generate(4, 4, Kind::Color, |x, _| match x {
                0 => [level, -0.0, -0.25, -0.0],
                1 => [-level, -0.0, -0.25, -0.0],
                _ => [f32::from_bits(0x8000_0001), level, 0.125, -0.0],
            }),
        );
    }
    let mut package = Package::new("Switch selection");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    for format in [OutputFormat::F16, OutputFormat::F32] {
        package.graphs[0].format = format;
        for (opacity, level) in [(0.0, 0.25), (1.0, 0.5)] {
            options.overrides.insert("weight".into(), Value::Float(opacity));
            for bound in [false, true, false] {
                let node = package.graphs[0].node_mut(blend).unwrap();
                node.params.insert("opacity".into(), Value::Float(opacity));
                if bound {
                    node.functions.insert("opacity".into(), FunctionGraph::input("weight", ValueType::Float));
                } else {
                    node.functions.remove("opacity");
                }
                let cooked = cook(&package, 0, &options, &cache);
                assert_eq!(cooked.nodes[&blend].error, None);
                assert_eq!(cooked.nodes[&blend].parameters["mode"], Value::Enum(7));
                let image = cooked.image(blend, 0).unwrap();
                let zero = if bound { 0.0_f32 } else { -0.0 };
                assert_eq!(image.get(0, 0, false).map(f32::to_bits), [level, zero, -0.25, zero].map(f32::to_bits), "{format:?} {opacity} bound {bound}");
                assert_eq!(image.get(1, 0, false).map(f32::to_bits), [-level, -0.0, -0.25, -0.0].map(f32::to_bits));
                let tiny = if !bound && format == OutputFormat::F32 { f32::from_bits(0x8000_0001) } else { zero };
                assert_eq!(image.get(2, 0, false).map(f32::to_bits), [tiny, level, 0.125, zero].map(f32::to_bits));
            }
        }
    }
}

#[test]
fn blend_preserves_the_selected_texture_levels_at_matching_precision() {
    use materialcraft_core::{FunctionGraph, OutputFormat, ParameterInheritance, ValueType};

    let (mut graph, input, stamp) = bitmap_impulse_graph();
    let blend = graph.add_node("blend", [0.0; 2]);
    graph.connect(input, "output", blend, "foreground").unwrap();
    graph.connect(blend, "output", stamp, "pattern0").unwrap();
    let mut package = Package::new("Selected impulse");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(32, 16, |x, y| f32::from(x == 10 && y == 8)));
    options.overrides.insert("coverage".into(), Value::Float(1.0));
    let cache = Mutex::new(Cache::default());
    for (mode, format, bound, expected) in [
        (7, OutputFormat::U16, false, [0, 2048, 12288, 2048, 0]),
        (7, OutputFormat::U16, true, [0, 1280, 5632, 1280, 0]),
        (0, OutputFormat::U16, false, [0, 2048, 12288, 2048, 0]),
        (7, OutputFormat::U8, false, [0, 1285, 5654, 1285, 0]),
        (7, OutputFormat::F32, false, [0, 1280, 5632, 1280, 0]),
    ] {
        let node = package.graphs[0].node_mut(blend).unwrap();
        node.params.insert("mode".into(), Value::Enum(mode));
        node.base.format_mode = ParameterInheritance::Absolute;
        node.base.format = format;
        if bound {
            node.functions.insert("opacity".into(), FunctionGraph::input("coverage", ValueType::Float));
        } else {
            node.functions.remove("opacity");
        }
        let result = cook(&package, 0, &options, &cache);
        assert!(result.nodes.values().all(|node| node.error.is_none()));
        let image = result.image(stamp, 0).unwrap();
        let levels = [30, 31, 32, 33, 34].map(|y| (image.get_gray(7, y, false) * 65535.0).round() as u16);
        assert_eq!(levels, expected);
    }
}

#[test]
fn bound_float_bitmap_reduces_close_texels_at_the_measured_filter_precision() {
    let mut graph = Graph::new("Row reduction");
    graph.size = [4, 0];
    graph.format = materialcraft_core::OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let mut package = Package::new("Row reduction");
    package.graphs.push(graph);
    for (width, row, sample, expected) in [(47, 15, 11, 0x3f00_001d), (65, 14, 9, 0x3f00_001d), (67, 14, 4, 0x3f00_001f)] {
        let image = Image::gray(width, 1, |x, _| f32::from_bits(0x3f00_0000 + (13 * x + 7 * row + 3 * x * row + row * row) % 64));
        let mut options = Options::new(&no_res);
        options.inputs.insert("input".into(), image);
        let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
        assert_eq!(result.image(input, 0).unwrap().get_gray(sample, 0, false).to_bits(), expected);
    }
}

#[test]
fn bound_float_bitmap_diffuses_half_rounding_after_resizing() {
    let mut graph = Graph::new("Half input");
    graph.size = [5, 5];
    graph.format = materialcraft_core::OutputFormat::F16;
    let input = graph.add_node("input_color", [0.0; 2]);
    let mut package = Package::new("Half input");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::filled(32, 17, Kind::Color, [1.0 + 1.0 / 2048.0, -1.0, 0.125, 0.12505]));
    let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
    let image = result.image(input, 0).unwrap();
    assert_eq!(image.get(0, 0, false), [1.0 + 1.0 / 1024.0, 0.0, 0.125, 0.125]);
    assert_eq!(image.get(1, 0, false), [1.0, 0.0, 0.125, 0.125]);
    assert_eq!(image.get(2, 0, false), [1.0 + 1.0 / 1024.0, 0.0, 0.125, 0.125 + 1.0 / 8192.0]);
}

#[test]
fn bitmap_node_filters_odd_rasters_at_their_original_channel_depth() {
    use materialcraft_core::{OutputFormat, Resource, ResourceBytes};

    let mut graph = Graph::new("Odd bitmap");
    graph.size = [5, 5];
    graph.format = OutputFormat::U16;
    let bitmap = graph.add_node("bitmap", [0.0; 2]);
    let node = graph.node_mut(bitmap).unwrap();
    node.params.insert("resource".into(), Value::Text("field".into()));
    node.params.insert("color_mode".into(), Value::Bool(false));
    let mut package = Package::new("Grayscale bitmap");
    package.graphs.push(graph);
    for (depth, expected) in [(8_u8, [11308, 30326, 40349, 49087]), (16, [419, 4749, 33094, 35028])] {
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&37_u32.to_be_bytes());
        header.extend_from_slice(&23_u32.to_be_bytes());
        header.extend([depth, 0, 0, 0, 0]);
        package.resources = vec![Resource::new("field", "field.png", ResourceBytes::new(header))];
        let field = Image::gray(37, 23, |x, y| {
            if depth == 8 {
                ((83 * x + 151 * y + 11 * x * y + 29) % 256) as f32 / 255.0
            } else {
                ((573 * x + 149 * y + 97 * x * y + 317) % 65536) as f32 / 65535.0
            }
        })
        .with_integer_sampling(true);
        let resource = |_: &str| Some(field.clone());
        let result = cook(&package, 0, &Options::new(&resource), &Mutex::new(Cache::default()));
        let image = result.image(bitmap, 0).unwrap();
        let levels = [(0, 0), (3, 7), (16, 16), (31, 31)].map(|(x, y)| (image.get_gray(x, y, false) * 65535.0).round() as u16);
        assert_eq!(levels, expected);
    }
}

#[test]
fn odd_color_bitmap_resources_keep_their_channel_depth_during_filtering() {
    use materialcraft_core::{OutputFormat, Resource, ResourceBytes};

    let mut graph = Graph::new("Odd color bitmap");
    graph.size = [4, 4];
    graph.format = OutputFormat::U16;
    let bitmap = graph.add_node("bitmap", [0.0; 2]);
    let node = graph.node_mut(bitmap).unwrap();
    node.params.insert("resource".into(), Value::Text("field".into()));
    node.params.insert("colour_space".into(), Value::Enum(1));
    let mut package = Package::new("Color bitmap");
    package.graphs.push(graph);

    for (width, height, depth, expected) in [
        (29_u32, 47_u32, 8, [[26985, 34181, 37522, 33924], [29555, 33667, 31354, 39321]]),
        (41, 27, 16, [[4100, 4730, 9310, 10498], [39835, 29231, 35098, 11450]]),
    ] {
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend([depth, 6, 0, 0, 0]);
        package.resources = vec![Resource::new("field", "field.png", ResourceBytes::new(header))];
        let levels = 1_u32 << depth;
        let field = Image::generate(width, height, Kind::Color, |x, y| {
            [(583, 191, 371, 3229), (47, 313, 59, 4511), (31, 693, 41, 8943), (997, 811, 97, 9043)]
                .map(|(a, b, c, d)| ((a * x + b * y + c * x * y + d) % levels) as f32 / (levels - 1) as f32)
        })
        .with_integer_sampling(true);
        let resource = |key: &str| (craft_texture::split_resource_key(key) == ("field", true)).then(|| field.clone());
        let result = cook(&package, 0, &Options::new(&resource), &Mutex::new(Cache::default()));
        let image = result.image(bitmap, 0).unwrap();
        assert_eq!([(0, 0), (6, 11)].map(|(x, y)| image.get(x, y, false).map(|v| (v * 65535.0).round() as u16)), expected);
    }
}

#[test]
fn bound_float_image_filters_an_odd_impulse_on_each_axis() {
    let mut graph = Graph::new("Impulse");
    graph.size = [4, 4];
    graph.format = materialcraft_core::OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let mut package = Package::new("Float input");
    package.graphs.push(graph);
    for (width, height, center) in [(3, 3, 0x3f524000), (35, 21, 0x3dbac261)] {
        let mut options = Options::new(&no_res);
        options.inputs.insert("input".into(), Image::gray(width, height, |x, y| f32::from(x == width / 2 && y == height / 2)));
        let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
        assert_eq!(result.image(input, 0).unwrap().get_gray(8, 8, false).to_bits(), center);
    }
}

#[test]
fn bound_float_image_with_an_odd_height_retains_its_stored_word_levels() {
    let mut graph = Graph::new("Odd height");
    graph.size = [5, 5];
    graph.format = materialcraft_core::OutputFormat::U16;
    let input = graph.add_node("input_color", [0.0; 2]);
    let mut package = Package::new("Word input");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::filled(32, 17, Kind::Color, [0.21875, 0.25, 0.5, 0.75]));
    let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
    let image = result.image(input, 0).unwrap();
    let levels = |x, y| image.get(x, y, false).map(|v| (v * 65535.0).round() as u16);
    assert_eq!(levels(0, 0), [14335, 16383, 32768, 49152]);
    assert_eq!(levels(1, 0), [14336, 16384, 32767, 49151]);
    assert_eq!(levels(0, 2), [14336, 16384, 32768, 49151]);
}

#[test]
fn bound_image_resizing_filters_normalized_levels_before_float_storage() {
    let mut graph = Graph::new("levels");
    graph.size = [4, 4];
    graph.format = materialcraft_core::OutputFormat::F32;
    let input = graph.add_node("input_color", [0.0; 2]);
    let mut package = Package::new("levels");
    package.graphs.push(graph);
    let stripe = Image::generate(8, 8, Kind::Color, |x, _| [(x % 2) as f32; 4]);
    let cache = Mutex::new(Cache::default());
    for (normalized, expected) in [(true, 16384.0 / 65535.0), (false, 0.25)] {
        let mut options = Options::new(&no_res);
        options.inputs.insert("input".into(), stripe.clone().with_integer_sampling(normalized));
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.image(input, 0).unwrap().get(1, 0, false), [expected; 4]);
        assert_eq!(result.image(input, 0).unwrap().get(0, 0, false), [0.0; 4]);
    }
}

#[test]
fn bound_image_resizing_filters_rgb_before_grayscale_conversion() {
    let mut graph = Graph::new("Grayscale input");
    graph.size = [4, 4];
    graph.format = materialcraft_core::OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let mut package = Package::new("Channel order");
    package.graphs.push(graph);
    let level = 1.0 / 65535.0;
    let stripe = Image::generate(8, 8, Kind::Color, |x, _| [if x % 2 == 1 { level } else { 0.0 }, 0.0, 0.0, 1.0]);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), stripe.with_integer_sampling(true));
    let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
    let image = result.image(input, 0).unwrap();
    assert_eq!(image.get(1, 0, false)[0], 0.0);
    assert_eq!(image.get(2, 0, false)[0], level * (1.0 / 3.0));
}

fn unchanged_size_after_read(getter: &str, variable: &str) -> materialcraft_core::FunctionGraph {
    let mut graph = materialcraft_core::FunctionGraph::default();
    let read = graph.add(getter, &[], Some(Value::Text(variable.into())));
    let zero = graph.add_constant(Value::Int2([0, 0]));
    graph.output = Some(graph.add("sequence", &[read, zero], None));
    graph
}

#[test]
fn filters_store_normalized_inputs_at_their_working_precision_when_one_size_is_parameter_controlled() {
    use materialcraft_core::{BaseParameter, OutputFormat, SizeMode};

    let mut graph = Graph::new("Filter sizing");
    graph.size = [4, 4];
    graph.format = OutputFormat::U8;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let source = graph.node_mut(input).unwrap();
    source.params.insert("identifier".into(), Value::Text("source".into()));
    source.base.size_mode = SizeMode::RelativeToParent;
    let levels = graph.add_node("levels", [64.0, 0.0]);
    let curve = graph.node_mut(levels).unwrap();
    curve.base.size_mode = SizeMode::RelativeToParent;
    curve.base.format = OutputFormat::F16;
    curve.params.insert("in_low".into(), Value::Float4([0.21875; 4]));
    curve.params.insert("in_high".into(), Value::Float4([0.46875; 4]));
    graph.connect(input, "output", levels, "input").unwrap();
    let sharpen = graph.add_node("sharpen", [64.0, 64.0]);
    let detail = graph.node_mut(sharpen).unwrap();
    detail.base.size_mode = SizeMode::RelativeToParent;
    detail.base.format = OutputFormat::F16;
    graph.connect(input, "output", sharpen, "input").unwrap();
    let [blur, fractional_half, fractional_float] =
        [(24.0, OutputFormat::F16), (32.0, OutputFormat::F16), (32.0, OutputFormat::F32)].map(|(intensity, format)| {
            let blur = graph.add_node("blur", [64.0, 128.0]);
            let smooth = graph.node_mut(blur).unwrap();
            smooth.base.size_mode = SizeMode::RelativeToParent;
            smooth.base.format = format;
            smooth.params.insert("intensity".into(), Value::Float(intensity));
            graph.connect(input, "output", blur, "input").unwrap();
            blur
        });
    let mut package = Package::new("Input precision");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    let field = Image::generate(16, 16, Kind::Gray, |x, y| {
        let byte: u8 = match (x, y) {
            (7, 8) => 5,
            (9, 8) => 78,
            (8, 7) => 31,
            (8, 9) => 42,
            _ => 58,
        };
        [f32::from(byte) / 255.0; 4]
    });
    options.inputs.insert("source".into(), field);
    options.overrides.insert("gain".into(), Value::Float(0.375));
    let cache = Mutex::new(Cache::default());
    let runtime = unchanged_size_after_read("get_float1", "gain");
    for (source_dynamic, output_dynamic) in [(false, false), (true, false), (false, true), (true, true)] {
        let [expected_levels, expected_sharpen, expected_blur, expected_half, expected_float] = if source_dynamic == output_dynamic {
            [0x3d0e8000, 0x3f1e8000, 0x3e3e0000, 0x3e572000, 0x3e673a97]
        } else {
            [0x3d0e0000, 0x3f1e6000, 0x3e3de000, 0x3e570000, 0x3e673a99]
        };
        for (id, dynamic) in [
            (input, source_dynamic),
            (levels, output_dynamic),
            (sharpen, output_dynamic),
            (blur, output_dynamic),
            (fractional_half, output_dynamic),
            (fractional_float, output_dynamic),
        ] {
            let functions = &mut package.graphs[0].node_mut(id).unwrap().base.functions;
            if dynamic {
                functions.insert(BaseParameter::OutputSize, runtime.clone());
            } else {
                functions.remove(&BaseParameter::OutputSize);
            }
        }
        let cooked = cook(&package, 0, &options, &cache);
        assert_eq!(cooked.nodes[&levels].error, None);
        assert_eq!(cooked.image(levels, 0).unwrap().get_gray(0, 0, false).to_bits(), expected_levels);
        assert_eq!(cooked.nodes[&sharpen].error, None);
        assert_eq!(cooked.image(sharpen, 0).unwrap().get_gray(9, 8, false).to_bits(), expected_sharpen);
        assert_eq!(cooked.nodes[&blur].error, None);
        assert_eq!(cooked.image(blur, 0).unwrap().get_gray(7, 8, false).to_bits(), expected_blur);
        assert_eq!(cooked.nodes[&fractional_half].error, None);
        assert_eq!(cooked.image(fractional_half, 0).unwrap().get_gray(6, 7, false).to_bits(), expected_half);
        assert_eq!(cooked.nodes[&fractional_float].error, None);
        assert_eq!(cooked.image(fractional_float, 0).unwrap().get_gray(6, 5, false).to_bits(), expected_float);
    }
}

#[test]
fn half_blends_store_integer_inputs_on_different_size_paths() {
    use materialcraft_core::{BaseParameter, OutputFormat, SizeMode};

    let mut graph = Graph::new("Blend sizing");
    graph.size = [1, 0];
    graph.format = OutputFormat::U8;
    let [foreground, background] = ["foreground", "background"].map(|identifier| {
        let id = graph.add_node("input_grayscale", [0.0; 2]);
        let node = graph.node_mut(id).unwrap();
        node.params.insert("identifier".into(), Value::Text(identifier.into()));
        node.base.size_mode = SizeMode::RelativeToParent;
        id
    });
    let bridge = graph.add_node("dot", [0.0; 2]);
    let blend = graph.add_node("blend", [0.0; 2]);
    let node = graph.node_mut(blend).unwrap();
    node.base.format = OutputFormat::F16;
    node.params.insert("opacity".into(), Value::Float(0.4));
    graph.connect(foreground, "output", bridge, "input").unwrap();
    graph.connect(bridge, "output", blend, "foreground").unwrap();
    graph.connect(background, "output", blend, "background").unwrap();
    let mut package = Package::new("Blend sizing");
    package.graphs.push(graph);

    let dimensions = unchanged_size_after_read("get_float2", "$size");
    let runtime = unchanged_size_after_read("get_float1", "gain");
    let mut options = Options::new(&no_res);
    options.overrides.insert("gain".into(), Value::Float(0.375));
    for (name, levels) in [("foreground", [71.0, 55.0]), ("background", [155.0, 1.0])] {
        options.inputs.insert(name.into(), Image::gray(2, 1, |x, _| levels[x as usize] / 255.0));
    }
    let cache = Mutex::new(Cache::default());
    for (front, back, inherit, expected) in [
        (false, false, false, [0x3ef3c000, 0x3db58000]),
        (true, false, false, [0x3ef3a000, 0x3db56000]),
        (false, true, false, [0x3ef38000, 0x3db58000]),
        (true, true, false, [0x3ef38000, 0x3db56000]),
        (false, true, true, [0x3ef3a000, 0x3db56000]),
        (true, true, true, [0x3ef3c000, 0x3db58000]),
    ] {
        let graph = &mut package.graphs[0];
        for (id, dynamic) in [(foreground, front), (background, back)] {
            graph.node_mut(id).unwrap().base.functions.insert(BaseParameter::OutputSize, if dynamic { runtime.clone() } else { dimensions.clone() });
        }
        graph.node_mut(blend).unwrap().base.size_mode = if inherit { SizeMode::RelativeToInput } else { SizeMode::RelativeToParent };
        let result = cook(&package, 0, &options, &cache);
        assert!(result.nodes.values().all(|node| node.error.is_none()));
        let pixels = result.image(blend, 0).unwrap();
        assert_eq!([0, 1].map(|x| pixels.get_gray(x, 0, false).to_bits()), expected);
        assert!(cook(&package, 0, &options, &cache).nodes[&blend].cached);
    }
}

#[test]
fn half_pattern_backgrounds_store_integer_inputs_on_different_size_paths() {
    use materialcraft_core::{BaseParameter, FunctionGraph, OutputFormat, PatternGraph, PatternNodeKind, SizeMode};

    let mut graph = Graph::new("Pattern background sizing");
    graph.size = [4, 4];
    graph.format = OutputFormat::U8;
    let background = graph.add_node("input_grayscale", [0.0; 2]);
    let draw = graph.add_node("fx_map", [0.0; 2]);
    let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let stamp = &mut pattern.nodes[0].parameters;
    stamp.insert("patterntype".into(), Value::Int(2));
    let level = 3.0 / 32768.0;
    stamp.insert("opacity".into(), Value::Float4([level, level, level, 1.0]));
    let node = graph.node_mut(draw).unwrap();
    node.base.size_mode = SizeMode::RelativeToParent;
    node.base.format = OutputFormat::F16;
    node.params.insert("color_mode".into(), Value::Bool(false));
    node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
    graph.connect(background, "output", draw, "background").unwrap();

    let mut package = Package::new("Pattern sizing");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(16, 16, |_, _| 58.0 / 255.0));
    options.overrides.insert("gain".into(), Value::Float(0.375));
    let cache = Mutex::new(Cache::default());
    let fixed = FunctionGraph::constant(Value::Int2([0, 0]));
    let system = unchanged_size_after_read("get_float2", "$sizelog2");
    let exposed = unchanged_size_after_read("get_float1", "gain");
    for (source_size, draw_size, expected) in [(&fixed, &system, 1864.0), (&fixed, &exposed, 1863.0), (&exposed, &fixed, 1863.0), (&exposed, &exposed, 1864.0)]
    {
        for (id, size) in [(background, source_size), (draw, draw_size)] {
            package.graphs[0].node_mut(id).unwrap().base.functions.insert(BaseParameter::OutputSize, size.clone());
        }
        let result = cook(&package, 0, &options, &cache);
        assert!(result.nodes.values().all(|node| node.error.is_none()));
        assert_eq!(result.image(draw, 0).unwrap().get_gray(8, 8, false), expected / 8192.0);
    }
}

#[test]
fn untiled_pattern_backgrounds_sample_directly_on_the_same_size_path() {
    use materialcraft_core::{BaseParameter, FunctionGraph, OutputFormat, ParameterInheritance, PatternGraph, PatternNodeKind, SizeMode};

    let mut graph = Graph::new("Resized pattern background");
    graph.size = [4, 5];
    graph.format = OutputFormat::U8;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let background = graph.add_node("fx_map", [0.0; 2]);
    let draw = graph.add_node("fx_map", [0.0; 2]);
    let levels = graph.add_node("levels", [0.0; 2]);
    for id in [input, background, levels] {
        let base = &mut graph.node_mut(id).unwrap().base;
        base.size_mode = SizeMode::RelativeToParent;
        base.size = [-1, -1];
    }
    for id in [background, draw] {
        let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
        if id == draw {
            let stamp = &mut pattern.nodes[0].parameters;
            stamp.insert("patterntype".into(), Value::Int(2));
            stamp.insert("opacity".into(), Value::Float4([0.10067749, 0.10067749, 0.10067749, 1.0]));
        }
        let node = graph.node_mut(id).unwrap();
        node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
        node.params.insert("color_mode".into(), Value::Bool(false));
    }
    graph.connect(input, "output", background, "background").unwrap();
    graph.connect(background, "output", levels, "input").unwrap();
    let mut package = Package::new("Pattern sample precision");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(8, 16, |x, y| ((47 * x + 83 * y + 11 * x * y + 849807) % 256) as f32 / 255.0));
    options.overrides.insert("gain".into(), Value::Float(0.375));
    let cache = Mutex::new(Cache::default());

    let fixed = FunctionGraph::constant(Value::Int2([0, 0]));
    let exposed = unchanged_size_after_read("get_float1", "gain");
    for (source, tiling, size, byte, half, corner) in [
        (input, [false, false], &fixed, 128.0, 0.5317383, 169.0),
        (background, [false, false], &fixed, 128.0, 0.5317383, 169.0),
        (background, [true, false], &fixed, 129.0, 0.53125, 171.0),
        (levels, [false, false], &fixed, 128.0, 0.5317383, 169.0),
        (levels, [true, true], &fixed, 129.0, 0.53125, 171.0),
        (background, [false, false], &exposed, 129.0, 0.53125, 171.0),
    ] {
        let graph = &mut package.graphs[0];
        graph.connect(source, "output", draw, "background").unwrap();
        let base = &mut graph.node_mut(source).unwrap().base;
        base.tiling_mode = ParameterInheritance::Absolute;
        base.tiling = tiling;
        let base = &mut graph.node_mut(draw).unwrap().base;
        base.tiling_mode = ParameterInheritance::Absolute;
        base.tiling = [true, true];
        base.size_mode = SizeMode::RelativeToParent;
        base.functions.insert(BaseParameter::OutputSize, size.clone());
        for (format, x, expected) in [(OutputFormat::U8, 2, byte / 255.0), (OutputFormat::F16, 4, half)] {
            package.graphs[0].node_mut(draw).unwrap().base.format = format;
            let result = cook(&package, 0, &options, &cache);
            assert!(result.nodes.values().all(|node| node.error.is_none()));
            let pixels = result.image(draw, 0).unwrap();
            assert_eq!(pixels.get_gray(x, 2, false), expected, "{format:?}");
            if format == OutputFormat::U8 {
                assert_eq!(pixels.get_gray(0, 0, false), corner / 255.0);
            }
        }
    }

    // Enlarge only Y. Repeating X still permits a direct sample.
    let graph = &mut package.graphs[0];
    graph.connect(input, "output", draw, "background").unwrap();
    graph.node_mut(input).unwrap().base.size = [0, -1];
    let draw_base = &mut graph.node_mut(draw).unwrap().base;
    draw_base.tiling = [false, false];
    draw_base.format = OutputFormat::U8;
    draw_base.functions.insert(BaseParameter::OutputSize, fixed);
    options.inputs.insert("input".into(), Image::gray(16, 16, |x, y| ((47 * x + 83 * y + 11 * x * y + 849807) % 256) as f32 / 255.0));
    for (source_tiling, byte) in [([true, false], 149.0), ([false, true], 150.0)] {
        package.graphs[0].node_mut(input).unwrap().base.tiling = source_tiling;
        let result = cook(&package, 0, &options, &cache);
        assert!(result.nodes.values().all(|node| node.error.is_none()));
        assert_eq!(result.image(draw, 0).unwrap().get_gray(2, 2, false), byte / 255.0);
    }
}

#[test]
fn connected_input_sizes_share_the_last_two_dimensional_filter_pass() {
    use materialcraft_core::{OutputFormat, SizeMode};
    let (mut parent, instance) = instancing("Parent", "Resize");
    let source = parent.add_node("input_color", [0.0; 2]);
    let source_base = &mut parent.node_mut(source).unwrap().base;
    source_base.size_mode = SizeMode::Absolute;
    source_base.format = OutputFormat::F32;
    parent.connect(source, "output", instance, "input").unwrap();
    let mut child = Graph::new("Resize");
    let bridge = child.add_node("input_color", [0.0; 2]);
    let inner_base = &mut child.node_mut(bridge).unwrap().base;
    inner_base.size_mode = SizeMode::Absolute;
    inner_base.size = [4, 3];
    inner_base.format = OutputFormat::F16;
    let output = child.add_node("output", [0.0; 2]);
    child.connect(bridge, "output", output, "input").unwrap();
    let mut package = Package::new("Rectangular input");
    package.graphs.extend([parent, child]);
    let mut options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    let high = 1.0 + 1.0 / 1024.0;
    for (size, red, green) in [([5, 5], high, 1.0), ([6, 4], 1.0, high), ([5, 4], 1.0, 1.0)] {
        package.graphs[0].node_mut(source).unwrap().base.size = size;
        let image = Image::generate(1 << size[0], 1 << size[1], Kind::Color, |x, y| {
            let r = if x % 2 == 0 { 1.0 } else { high };
            let g = if y % 2 == 0 { 1.0 } else { high };
            [r, g, -r, -g]
        });
        options.inputs.insert("input".into(), image);
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&instance].error, None);
        let image = result.image(instance, 0).unwrap();
        assert_eq!((image.width(), image.height()), (16, 8));
        assert!(
            image.data().as_chunks::<4>().0.iter().all(|&pixel| pixel == [red, green, -red, -green]),
            "source {size:?}, first {:?}",
            image.get(0, 0, false)
        );
    }
}

#[test]
fn an_image_bridge_resolves_its_size_against_its_own_wire() {
    use materialcraft_core::SizeMode;

    let (mut parent, connected) = instancing("Parent", "Bridge");
    let unconnected = parent.add_node("graph_instance", [0.0, 80.0]);
    parent.node_mut(unconnected).unwrap().params.insert("graph".into(), Value::Text("Bridge".into()));
    for id in [connected, unconnected] {
        let base = &mut parent.node_mut(id).unwrap().base;
        base.size_mode = SizeMode::Absolute;
        base.size = [6; 2];
    }
    let input = parent.add_node("input_grayscale", [0.0; 2]);
    let source = &mut parent.node_mut(input).unwrap().base;
    source.size_mode = SizeMode::Absolute;
    source.size = [4, 5];
    parent.connect(input, "output", connected, "input").unwrap();

    let mut child = Graph::new("Bridge");
    let bridge = child.add_node("input_grayscale", [0.0; 2]);
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(bridge, "output", output, "input").unwrap();
    let mut package = Package::new("Bridge sizes");
    package.graphs.extend([parent, child]);
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (mode, size, expected) in [
        (SizeMode::RelativeToInput, [0, 0], [(16, 32), (64, 64)]),
        (SizeMode::RelativeToInput, [-1, 1], [(8, 64), (32, 128)]),
        (SizeMode::RelativeToParent, [0, 0], [(64, 64); 2]),
        (SizeMode::Absolute, [5, 4], [(32, 16); 2]),
    ] {
        let base = &mut package.graphs[1].node_mut(bridge).unwrap().base;
        base.size_mode = mode;
        base.size = size;
        let cooked = cook(&package, 0, &options, &cache);
        let dimensions = [connected, unconnected].map(|id| {
            assert_eq!(cooked.nodes[&id].error, None);
            let image = cooked.image(id, 0).unwrap();
            (image.width(), image.height())
        });
        assert_eq!(dimensions, expected);
    }
}

#[test]
fn bound_image_resizing_reads_the_upstream_float_format_inside_instances() {
    use materialcraft_core::{OutputFormat, SizeMode};
    let (mut parent, instance) = instancing("Parent", "Resized");
    let source = parent.add_node("input_grayscale", [0.0; 2]);
    let base = &mut parent.node_mut(source).unwrap().base;
    base.size_mode = SizeMode::Absolute;
    base.size = [3, 3];
    parent.connect(source, "output", instance, "input").unwrap();
    let mut child = Graph::new("Resized");
    child.absolute_output_size = Some([4, 4]);
    let bridge = child.add_node("input_grayscale", [0.0; 2]);
    child.node_mut(bridge).unwrap().base.size_mode = SizeMode::RelativeToParent;
    child.node_mut(bridge).unwrap().base.format = OutputFormat::F32;
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(bridge, "output", output, "input").unwrap();
    let mut package = Package::new("Stored levels");
    package.graphs.extend([parent, child]);
    let corners = [0x3fd0e000, 0xbfa46000, 0xc02d8000, 0x407f0000];
    let image = Image::gray(8, 8, |x, y| f32::from_bits(corners[((y % 2) * 2 + x % 2) as usize]));
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), image);
    let cache = Mutex::new(Cache::default());
    for (format, expected) in [(OutputFormat::F32, 0x3ed5f000), (OutputFormat::F16, 0x3ed60000), (OutputFormat::F32, 0x3ed5f000)] {
        package.graphs[0].node_mut(source).unwrap().base.format = format;
        let cooked = cook(&package, 0, &options, &cache);
        assert_eq!(cooked.nodes[&instance].error, None);
        assert_eq!(cooked.image(instance, 0).unwrap().get(1, 1, false)[0].to_bits(), expected);
    }
}

#[test]
fn wired_input_sampling_keeps_the_source_texture_and_stores_the_direct_output() {
    use materialcraft_core::{FunctionGraph, OutputFormat, ParameterInheritance, SizeMode};
    let (mut parent, instance) = instancing("Parent", "TwoReads");
    let source = parent.add_node("input_grayscale", [0.0; 2]);
    let base = &mut parent.node_mut(source).unwrap().base;
    base.size_mode = SizeMode::Absolute;
    base.size = [3, 3];
    base.format = OutputFormat::U8;
    parent.connect(source, "output", instance, "input").unwrap();
    let mut child = Graph::new("TwoReads");
    child.absolute_output_size = Some([4, 4]);
    let bridge = child.add_node("input_grayscale", [0.0; 2]);
    child.node_mut(bridge).unwrap().base.size_mode = SizeMode::RelativeToParent;
    child.node_mut(bridge).unwrap().base.format = OutputFormat::F16;
    let reader = child.add_node("pixel_processor", [0.0; 2]);
    let node = child.node_mut(reader).unwrap();
    node.base.format = OutputFormat::F32;
    node.params.insert("color_mode".into(), Value::Bool(false));
    child.connect(bridge, "output", reader, "input0").unwrap();
    for (name, source) in [("output", reader), ("passed", bridge)] {
        let output = child.add_node("output", [0.0; 2]);
        child.node_mut(output).unwrap().params.insert("identifier".into(), Value::Text(name.into()));
        child.connect(source, "output", output, "input").unwrap();
    }
    let mut package = Package::new("Wired sampling");
    package.graphs.extend([parent, child]);
    let mut options = Options::new(&no_res);
    options.inputs.insert("input".into(), Image::gray(8, 8, |x, _| (x % 2) as f32));
    let cache = Mutex::new(Cache::default());
    for (filter, tile, sample, direct_edge) in [(0, true, 0.0, 0.25), (1, true, 16384.0 / 65535.0, 0.25), (1, false, 16384.0 / 65535.0, 0.0)] {
        let base = &mut package.graphs[1].node_mut(bridge).unwrap().base;
        base.tiling_mode = ParameterInheritance::Absolute;
        base.tiling = [tile, false];
        let mut function = FunctionGraph::default();
        let pos = function.add("get_float2", &[], Some(Value::Text("$pos".into())));
        function.output = Some(function.add("samplelum", &[pos], Some(Value::Int2([0, filter]))));
        package.graphs[1].node_mut(reader).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
        let cooked = cook(&package, 0, &options, &cache);
        assert_eq!(cooked.nodes[&instance].error, None);
        assert_eq!(cooked.image(instance, 0).unwrap().get(1, 0, false)[0], sample);
        assert_eq!(cooked.image(instance, 1).unwrap().get(0, 0, false)[0], direct_edge);
    }
}

#[test]
fn wired_value_and_pattern_functions_read_the_callers_texture() {
    use materialcraft_core::{FunctionGraph, OutputFormat, PatternGraph, PatternNodeKind};
    let (mut parent, instance) = instancing("Parent", "Sample");
    parent.size = [5; 2];
    let source = parent.add_node("input_color", [0.0; 2]);
    parent.node_mut(source).unwrap().base.format = OutputFormat::F32;
    parent.connect(source, "output", instance, "input").unwrap();

    let mut child = Graph::new("Sample");
    let bridge = child.add_node("input_color", [0.0; 2]);
    child.node_mut(bridge).unwrap().base.format = OutputFormat::U16;
    let mut function = FunctionGraph::default();
    let coord = function.add_constant(Value::Float2([0.375, 0.6875]));
    function.output = Some(function.add("samplecol", &[coord], Some(Value::Int2([0, 1]))));
    let reader = child.add_node("value_processor", [0.0; 2]);
    child.node_mut(reader).unwrap().params.insert("function".into(), Value::Function(Box::new(function.clone())));
    child.connect(bridge, "output", reader, "input0").unwrap();

    let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
    pattern.nodes[0].parameters.insert("patterntype".into(), Value::Int(2));
    pattern.nodes[0].parameters.insert("opacity".into(), Value::Function(Box::new(function)));
    let draw = child.add_node("fx_map", [0.0; 2]);
    let node = child.node_mut(draw).unwrap();
    node.base.format = OutputFormat::F32;
    node.params.insert("color_mode".into(), Value::Bool(true));
    node.params.insert("background".into(), Value::Color([0.0; 4]));
    node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
    child.connect(bridge, "output", draw, "pattern0").unwrap();
    for (name, source) in [("output", draw), ("sample", reader)] {
        let output = child.add_node("output", [0.0; 2]);
        child.node_mut(output).unwrap().params.insert("identifier".into(), Value::Text(name.into()));
        child.connect(source, "output", output, "input").unwrap();
    }
    let mut package = Package::new("Wired color sampling");
    package.graphs.extend([parent, child]);
    let mut options = Options::new(&no_res);
    let field = Image::generate(32, 32, Kind::Color, |x, y| {
        std::array::from_fn(|i| ((37 * x + 41 * y + 7 * x * y + 53 * 19 + 61 * i as u32 + 5 * x * i as u32) % 509) as f32 / 508.0)
    });
    options.inputs.insert("input".into(), field);
    let result = cook(&package, 0, &options, &Mutex::new(Cache::default()));
    assert_eq!(result.nodes[&instance].error, None);
    let color = [0.44734252, 0.43011808, 0.4128937, 0.64616144];
    assert_eq!(result.value(instance, 1), Some(&Value::Float4(color)));
    let drawn = [0.2890555, 0.27792573, 0.266796, 0.64616144];
    assert_eq!(result.image(instance, 0).unwrap().get(0, 0, false), drawn);
}

#[test]
fn value_processor_reads_the_upstream_channel_format_after_a_cache_hit() {
    use materialcraft_core::{FunctionGraph, OutputFormat};

    let mut function = FunctionGraph::default();
    let uv = function.add_constant(Value::Float2([0.5, 0.5]));
    function.output = Some(function.add("samplelum", &[uv], Some(Value::Int2([0, 0]))));
    let mut graph = Graph::new("Value sample");
    graph.size = [4; 2];
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let processor = graph.add_node("value_processor", [0.0; 2]);
    graph.node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    graph.connect(input, "output", processor, "input0").unwrap();
    let mut package = Package::new("Value sample");
    package.graphs.push(graph);
    let mut options = Options::new(&no_res);
    let channel = 222.0_f32 / 255.0;
    options.inputs.insert("input".into(), Image::gray(16, 16, |_, _| channel));
    let cache = Mutex::new(Cache::default());
    for (format, expected) in [(OutputFormat::U8, 222.0 * (1.0 / 255.0)), (OutputFormat::F32, channel)] {
        package.graphs[0].node_mut(input).unwrap().base.format = format;
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&processor].error, None);
        assert_eq!(result.value(processor, 0), Some(&Value::Float(expected)));
        let cached = cook(&package, 0, &options, &cache);
        assert!(cached.nodes[&processor].cached);
        assert_eq!(cached.value(processor, 0), Some(&Value::Float(expected)));
    }
}

#[test]
fn input_nodes_read_bound_preview_images() {
    let mut p = Package::new("p");
    let mut g = Graph::new("g");
    g.size = [4, 4];
    let i = g.add_node("input_grayscale", [0.0, 0.0]);
    g.node_mut(i).unwrap().params.insert("identifier".into(), Value::Text("curvature".into()));
    let o = g.add_node("output", [100.0, 0.0]);
    g.connect(i, "output", o, "input").unwrap();
    g.input_images.insert("curvature".into(), "curv".into());
    p.graphs.push(g);
    let res = |n: &str| (n == "curv").then(|| Image::filled(4, 4, Kind::Color, [0.8, 0.8, 0.8, 1.0]));
    let cache = Mutex::new(Cache::default());
    let c = cook(&p, 0, &Options::new(&res), &cache);
    let img = c.image(o, 0).unwrap();

    assert!(img.is_gray() && (img.data()[0] - 0.8).abs() < 1e-4, "bound image, as gray");

    // The caller's image wins over the bound one
    let mut opts = Options::new(&res);
    opts.inputs.insert("curvature".into(), Image::filled(4, 4, Kind::Gray, [0.1; 4]));
    let c = cook(&p, 0, &opts, &cache);

    assert!((c.image(o, 0).unwrap().data()[0] - 0.1).abs() < 1e-4);
}

#[test]
fn input_change_recooks_only_readers() {
    use materialcraft_core::{FunctionGraph, FunctionNode};
    let (mut p, noise, blur, out) = sample();
    let reader = p.graphs[0].add_node("pixel_processor", [0.0, 100.0]);
    let get_a = FunctionNode {
        missing: None,
        missing_data: String::new(),
        instance: None,
        source: None,
        id: 1,
        type_code: 0,
        op: "get_float1".into(),
        inputs: vec![],
        value: Some(Value::Text("a".into())),
        pos: [0.0; 2],
    };
    p.graphs[0]
        .node_mut(reader)
        .unwrap()
        .params
        .insert("function".into(), Value::Function(Box::new(FunctionGraph { nodes: vec![get_a], output: Some(1), ..Default::default() })));
    let cache = &Mutex::new(Cache::default());
    let mut opts = Options::new(&no_res);
    opts.overrides.insert("a".into(), Value::Float(0.25));
    let first = cook(&p, 0, &opts, cache);
    assert!(first.nodes.values().all(|r| r.error.is_none() && !r.cached));
    opts.overrides.insert("a".into(), Value::Float(0.75));
    let second = cook(&p, 0, &opts, cache);
    for id in [noise, blur, out] {
        assert!(second.nodes[&id].cached, "node {} does not read `a`", id.0);
    }
    assert!(!second.nodes[&reader].cached, "the reader must re-cook");
}

#[test]
fn partial_cooks_keep_shared_dependencies_and_first_duplicate_node() {
    let mut graph = Graph::new("Partial cook");
    graph.size = [4, 4];
    let source = graph.add_node("uniform", [0.0; 2]);
    let first = graph.add_node("dot", [0.0; 2]);
    let second = graph.add_node("dot", [0.0; 2]);
    let unrelated = graph.add_node("uniform", [0.0; 2]);
    graph.connect(source, "output", first, "input").unwrap();
    graph.connect(source, "output", second, "input").unwrap();

    let mut duplicate = graph.node(source).unwrap().clone();
    duplicate.def = "input_value".into();
    graph.nodes.push(duplicate);
    let mut package = Package::new("Partial cook");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    let mut options = Options::new(&no_res);
    options.only = Some(vec![first, second]);

    let cooked = cook(&package, 0, &options, &cache);
    assert_eq!(cooked.nodes.len(), 3);
    assert!(!cooked.nodes.contains_key(&unrelated));
    for id in [source, first, second] {
        assert!(cooked.nodes[&id].error.is_none());
        assert!(cooked.image(id, 0).is_some());
    }
}

use crate::cache_key::image_key;
use std::sync::Arc;
#[test]
fn cached_input_fingerprint_tracks_copy_on_write_and_shape() {
    let cache = Mutex::new(Cache::default());
    let image = Image::filled(128, 128, Kind::Gray, [0.0; 4]);
    let first = image_key(&image, &cache);

    assert_eq!(image_key(&image, &cache), first);

    let mut data = image.data().to_vec();
    data[1] = 1.0;
    let image = Image::from_data(128, 128, Kind::Gray, data, &Limits::default()).unwrap();
    let changed = image_key(&image, &cache);

    assert_ne!(changed, first);

    // The same buffer read as another shape
    let image = Image::from_data(64, 256, Kind::Gray, Arc::clone(image.shared_data()), &Limits::default()).unwrap();
    let reshaped = image_key(&image, &cache);

    assert_ne!(reshaped, changed);

    cache.lock().unwrap().clear();

    assert_eq!(image_key(&image, &cache), reshaped);
}

#[test]
fn preview_cap_does_not_compound_downscaling() {
    let (mut p, noise, blur, out) = sample();
    p.graphs[0].size = [10, 10];
    p.graphs[0].node_mut(blur).unwrap().base.size = [-3, -3];
    let cache = Mutex::new(Cache::default());
    let mut opts = Options::new(&no_res);
    opts.max_size = Some(512);
    let c = cook(&p, 0, &opts, &cache);

    assert_eq!(c.image(noise, 0).map(|i| (i.width(), i.height())), Some((512, 512)));
    assert_eq!(c.image(blur, 0).map(|i| (i.width(), i.height())), Some((128, 128)));
    assert_eq!(c.image(out, 0).map(|i| (i.width(), i.height())), Some((128, 128)));
}

#[test]
fn preview_cap_uses_nominal_dimensions_through_bypassed_nodes() {
    let (mut p, noise, blur, out) = sample();
    p.graphs[0].size = [13, 12];
    p.graphs[0].node_mut(blur).unwrap().disabled = true;
    p.graphs[0].node_mut(out).unwrap().base.size = [1, 0];
    let cache = Mutex::new(Cache::default());
    let mut opts = Options::new(&no_res);
    opts.max_size = Some(128);
    let c = cook(&p, 0, &opts, &cache);

    assert_eq!(c.image(noise, 0).map(|i| (i.width(), i.height())), Some((128, 64)));
    assert_eq!(c.image(blur, 0).map(|i| (i.width(), i.height())), Some((128, 64)));
    assert_eq!(c.image(out, 0).map(|i| (i.width(), i.height())), Some((128, 64)));
}

#[test]
fn changed_unsampled_input_recooks() {
    let mut g = Graph::new("g");
    g.size = [7, 7];
    let input = g.add_node("input_grayscale", [0.0, 0.0]);
    let mut p = Package::new("p");
    p.graphs.push(g);
    let mut opts = Options::new(&no_res);
    let image = Image::filled(128, 128, Kind::Gray, [0.0; 4]);
    opts.inputs.insert("input".into(), image.clone());
    let cache = Mutex::new(Cache::default());
    let first = cook(&p, 0, &opts, &cache);

    assert!(!first.nodes[&input].cached);

    let mut data = image.data().to_vec();
    data[1] = 1.0;
    let changed = Image::from_data(128, 128, Kind::Gray, data, &Limits::default()).unwrap();
    opts.inputs.insert("input".into(), changed);
    let second = cook(&p, 0, &opts, &cache);

    assert!(!second.nodes[&input].cached);
    assert_eq!(second.image(input, 0).unwrap().data()[1], 1.0);
}

#[test]
fn linear_input_images_and_bitmap_nodes_request_data_reads() {
    // The decoder returns 0.5 for a linear request and 0.2 for an sRGB one
    let res = |k: &str| {
        let (name, linear) = craft_texture::split_resource_key(k);
        (name == "m").then(|| Image::filled(4, 4, Kind::Color, [if linear { 0.5 } else { 0.2 }; 4]))
    };

    let mut p = Package::new("p");
    let mut g = Graph::new("g");
    g.size = [4, 4];
    g.format = materialcraft_core::OutputFormat::F32;
    let i = g.add_node("input_color", [0.0, 0.0]);
    g.node_mut(i).unwrap().params.insert("identifier".into(), Value::Text("n".into()));
    let b = g.add_node("bitmap", [0.0, 100.0]);
    g.node_mut(b).unwrap().params.insert("resource".into(), Value::Text("m".into()));
    g.input_images.insert("n".into(), "m".into());
    p.graphs.push(g);
    let first = |p: &Package| {
        let cache = Mutex::new(Cache::default());
        let c = cook(p, 0, &Options::new(&res), &cache);
        (c.image(i, 0).unwrap().data()[0], c.image(b, 0).unwrap().data()[0])
    };

    assert_eq!(first(&p), (0.2, 0.5), "the bitmap defaults to file channels");

    p.graphs[0].node_mut(b).unwrap().params.insert("colour_space".into(), Value::Enum(0));
    assert_eq!(first(&p), (0.2, 0.2), "an explicit sRGB read still decodes the color");

    p.graphs[0].input_images_linear.insert("n".into());
    p.graphs[0].node_mut(b).unwrap().params.insert("colour_space".into(), Value::Enum(1));

    assert_eq!(first(&p), (0.5, 0.5));
}

#[test]
fn bound_input_image_resource_change_recooks_instance() {
    use materialcraft_core::{Resource, ResourceBytes};
    use std::sync::atomic::AtomicU8;
    let mut sub = Graph::new("sub");
    sub.size = [4, 4];
    let i = sub.add_node("input_grayscale", [0.0, 0.0]);
    sub.node_mut(i).unwrap().params.insert("identifier".into(), Value::Text("in".into()));
    let o = sub.add_node("output", [100.0, 0.0]);
    sub.connect(i, "output", o, "input").unwrap();
    sub.input_images.insert("in".into(), "curv".into());
    let (top, inst) = instancing("top", "sub");
    let mut p = Package::new("p");
    p.graphs = vec![top, sub];
    p.resources.push(Resource::new("curv", "curv.bin", ResourceBytes::new(vec![1; 100])));

    // Stands in for decoding: the first byte picks the shade
    let shade = AtomicU8::new(1);
    let res = |_: &str| Some(Image::filled(4, 4, Kind::Gray, [f32::from(shade.load(Ordering::Relaxed)) / 10.0; 4]));
    let cache = Mutex::new(Cache::default());
    let opts = Options::new(&res);
    let first = cook(&p, 0, &opts, &cache);

    assert!(first.nodes[&inst].error.is_none(), "{:?}", first.nodes[&inst].error);
    assert!(!first.nodes[&inst].cached);

    let unchanged = cook(&p, 0, &opts, &cache);

    assert!(unchanged.nodes[&inst].cached, "an unchanged instance should be served from the cache");

    p.resources[0].bytes = ResourceBytes::new(vec![2; 100]);
    shade.store(2, Ordering::Relaxed);
    let second = cook(&p, 0, &opts, &cache);

    assert!(!second.nodes[&inst].cached, "changing the bound input image resource must recook the instance");
}

#[test]
fn bound_input_image_linear_flag_change_recooks_instance() {
    use materialcraft_core::{Resource, ResourceBytes};
    let mut sub = Graph::new("sub");
    sub.size = [4, 4];
    let i = sub.add_node("input_grayscale", [0.0, 0.0]);
    sub.node_mut(i).unwrap().params.insert("identifier".into(), Value::Text("in".into()));
    let o = sub.add_node("output", [100.0, 0.0]);
    sub.connect(i, "output", o, "input").unwrap();
    sub.input_images.insert("in".into(), "curv".into());
    let (top, inst) = instancing("top", "sub");
    let mut p = Package::new("p");
    p.graphs = vec![top, sub];
    p.resources.push(Resource::new("curv", "curv.bin", ResourceBytes::new(vec![1; 100])));
    let res = |k: &str| {
        let (name, linear) = craft_texture::split_resource_key(k);
        (name == "curv").then(|| Image::filled(4, 4, Kind::Gray, [if linear { 0.5 } else { 0.2 }; 4]))
    };
    let cache = Mutex::new(Cache::default());
    let opts = Options::new(&res);
    let first = cook(&p, 0, &opts, &cache);

    assert!(first.nodes[&inst].error.is_none(), "{:?}", first.nodes[&inst].error);
    assert!(!first.nodes[&inst].cached);

    let first_value = first.image(inst, 0).unwrap().data()[0];

    assert!((first_value - 0.2).abs() < 1e-4, "sRGB bound image should reach the instance, got {first_value}");

    let second = cook(&p, 0, &opts, &cache);

    assert!(second.nodes[&inst].cached, "an unchanged instance should be served from the cache");

    p.graphs[1].input_images_linear.insert("in".into());
    let third = cook(&p, 0, &opts, &cache);

    assert!(!third.nodes[&inst].cached, "toggling input_images_linear must recook the instance");

    let third_value = third.image(inst, 0).unwrap().data()[0];

    assert!((third_value - 0.5).abs() < 1e-4, "linear bound image should reach the instance, got {third_value}");
}

#[test]
fn cooked_parameters_retain_values_without_processor_graph_copies() {
    let mut graph = Graph::new("Parameter results");
    graph.size = [4, 4];
    for _ in 0..12 {
        let owner = graph.add_node("value_processor", [0.0; 2]);
        let mut function = materialcraft_core::FunctionGraph::constant(Value::Float(0.25));
        let constant = function.output.unwrap();
        for _ in 0..32 {
            function.output = Some(function.add("add", &[function.output.unwrap(), constant], None));
        }
        graph.node_mut(owner).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    }
    let scalar = graph.add_node("uniform", [0.0; 2]);
    graph.node_mut(scalar).unwrap().functions.insert("color".into(), materialcraft_core::FunctionGraph::constant(Value::Color([0.5; 4])));
    let mut package = Package::new("Parameter results");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());

    let results = [cook(&package, 0, &Options::new(&no_res), &cache), cook(&package, 0, &Options::new(&no_res), &cache)];

    assert!(results[1].nodes.values().all(|result| result.cached));
    for result in results {
        for (owner, node) in &result.nodes {
            assert_eq!(node.error, None);
            assert!(
                node.parameters.values().all(|value| materialcraft_core::ValueType::of(value).is_some()),
                "cooked previews must not retain authoring graphs"
            );
            if *owner != scalar {
                assert_eq!(result.value(*owner, 0), Some(&Value::Float(8.25)));
            }
        }
        assert_eq!(result.nodes[&scalar].parameters.get("color"), Some(&Value::Color([0.5; 4])));
    }
}

#[test]
fn a_pattern_uses_only_its_background_for_input_relative_size_and_precision() {
    use materialcraft_core::{OutputFormat, ParameterInheritance, SizeMode};

    let mut graph = Graph::new("Pattern inputs");
    graph.size = [5, 4];
    graph.format = OutputFormat::U8;
    let stamp = graph.add_node("uniform", [0.0; 2]);
    let back = graph.add_node("uniform", [0.0; 2]);
    for (id, size, format) in [(stamp, [6; 2], OutputFormat::F32), (back, [4, 5], OutputFormat::U16)] {
        let base = &mut graph.node_mut(id).unwrap().base;
        base.size_mode = SizeMode::Absolute;
        base.size = size;
        base.format = format;
    }
    let draw = graph.add_node("fx_map", [100.0, 0.0]);
    graph.node_mut(draw).unwrap().base.format_mode = ParameterInheritance::RelativeToInput;
    graph.connect(stamp, "output", draw, "pattern0").unwrap();
    graph.connect(back, "output", draw, "background").unwrap();
    let mut package = Package::new("Pattern input inheritance");
    package.graphs.push(graph);
    let cache = Mutex::new(Cache::default());
    let result = |package: &Package| {
        let cooked = cook(package, 0, &Options::new(&no_res), &cache);
        let item = &cooked.nodes[&draw];
        assert_eq!(item.error, None);
        (item.base.size, item.base.format)
    };
    assert_eq!(result(&package), ([16, 32], OutputFormat::U16));
    package.graphs[0].node_mut(draw).unwrap().base.size = [-1, 1];
    assert_eq!(result(&package), ([8, 64], OutputFormat::U16));
    assert!(package.graphs[0].disconnect(draw, "background"));
    assert_eq!(result(&package), ([16, 32], OutputFormat::U8));
    // Bypassing this draw still passes through the available stamp texture.
    package.graphs[0].node_mut(draw).unwrap().disabled = true;
    assert_eq!(result(&package), ([64, 64], OutputFormat::F32));
}

#[test]
fn enabled_nodes_resolve_size_from_the_graph_or_their_primary_image() {
    use materialcraft_core::{BaseParameter, FunctionGraph, OutputFormat, ParameterInheritance, SizeMode, ValueType};

    for (definition, port, size, preview) in
        [("pixel_processor", "input0", [64_u32, 32], (16, 8)), ("svg", "background", [64, 32], (16, 8)), ("fx_map", "background", [32, 64], (8, 16))]
    {
        for dynamic in [false, true] {
            let mut graph = Graph::new("Generator sizes");
            graph.size = [6, 5];
            let background = graph.add_node("uniform", [0.0; 2]);
            let base = &mut graph.node_mut(background).unwrap().base;
            base.size_mode = SizeMode::Absolute;
            base.size = [5, 6];
            base.seed = 17;
            base.format = OutputFormat::F32;
            base.tiling = [false, true];
            base.tiling_mode = ParameterInheritance::Absolute;
            base.pixel_ratio = 1;
            base.pixel_ratio_mode = ParameterInheritance::Absolute;
            base.pixel_size = [0.25, 0.5];
            base.pixel_size_mode = ParameterInheritance::Absolute;
            let generator = graph.add_node(definition, [200.0, 0.0]);
            let node = graph.node_mut(generator).unwrap();
            node.base.format_mode = ParameterInheritance::RelativeToInput;
            if definition == "pixel_processor" {
                node.params.insert("function".into(), Value::Function(Box::new(FunctionGraph::constant(Value::Float4([0.25; 4])))));
            }
            if dynamic {
                let mut function = FunctionGraph::input("$sizelog2", ValueType::Float2);
                let actual = function.output.unwrap();
                let expected = function.add_constant(Value::Float2(size.map(|axis| axis.ilog2() as f32)));
                let difference = function.add("sub", &[actual, expected], None);
                function.output = Some(function.add("toint2", &[difference], None));
                graph.node_mut(generator).unwrap().base.functions.insert(BaseParameter::OutputSize, function);
            }
            graph.connect(background, "output", generator, port).unwrap();
            let mut package = Package::new("Generator sizes");
            package.graphs.push(graph);
            let cache = Mutex::new(Cache::default());
            let mut options = Options::new(&no_res);
            options.max_size = Some(16);

            let cooked = cook(&package, 0, &options, &cache);
            let result = &cooked.nodes[&generator];

            assert_eq!(result.error, None, "{definition}");
            assert_eq!(result.base.size, size, "{definition}, dynamic: {dynamic}");
            assert_eq!(cooked.image(generator, 0).map(|image| (image.width(), image.height())), Some(preview));
            assert_eq!(result.base.seed, 0);
            assert_eq!(result.base.format, OutputFormat::F32);
            assert_eq!(result.base.tiling, [false, true]);
            assert_eq!(result.base.pixel_ratio, 1);
            assert_eq!(result.base.pixel_size, [0.25, 0.5]);

            if !dynamic {
                assert_eq!(
                    node_size(&package.graphs[0], materialcraft_nodes::find(definition), package.graphs[0].node(generator).unwrap(), Some((32, 64)), Some(16)),
                    preview
                );
            }

            package.graphs[0].node_mut(generator).unwrap().disabled = true;
            let cooked = cook(&package, 0, &options, &cache);

            assert_eq!(cooked.nodes[&generator].error, None);
            assert_eq!(cooked.nodes[&generator].base.size, cooked.nodes[&background].base.size);
            assert_eq!(cooked.image(generator, 0).map(|image| (image.width(), image.height())), Some((8, 16)));
            assert_eq!(
                node_size(&package.graphs[0], materialcraft_nodes::find(definition), package.graphs[0].node(generator).unwrap(), Some((32, 64)), Some(16)),
                (8, 16)
            );
        }
    }
}

#[test]
fn variables_set_by_a_base_parameter_function_reach_the_nodes_other_functions() {
    use materialcraft_core::{BaseParameter, FunctionGraph};

    let mut size = FunctionGraph::default();
    let grid = size.add_constant(Value::Int(2));
    let store = size.add("set", &[grid], Some(Value::Text("grid".into())));
    let unchanged = size.add_constant(Value::Int2([0, 0]));
    size.output = Some(size.add("sequence", &[store, unchanged], None));

    let mut roughness = FunctionGraph::default();
    let read = roughness.add("get_integer1", &[], Some(Value::Text("grid".into())));
    roughness.output = Some(roughness.add("tofloat", &[read], None));

    let mut graph = Graph::new("Shared variables");
    let generator = graph.add_node("fx_map", [0.0; 2]);
    let node = graph.node_mut(generator).unwrap();
    node.base.functions.insert(BaseParameter::OutputSize, size);
    node.functions.insert("roughness".into(), roughness);
    let mut package = Package::new("Shared variables");
    package.graphs.push(graph);

    let cooked = cook(&package, 0, &Options::new(&no_res), &Mutex::new(Cache::default()));
    let result = &cooked.nodes[&generator];

    assert_eq!(result.error, None);
    assert_eq!(result.parameters.get("roughness"), Some(&Value::Float(2.0)));
}

#[test]
fn a_color_parameter_function_may_return_a_gray_level_or_an_rgb_triple() {
    use materialcraft_core::FunctionGraph;

    for (value, expected) in [(Value::Float(0.25), [0.25, 0.25, 0.25, 1.0]), (Value::Float3([0.1, 0.2, 0.3]), [0.1, 0.2, 0.3, 1.0])] {
        let mut graph = Graph::new("Color function");
        let uniform = graph.add_node("uniform", [0.0; 2]);
        graph.node_mut(uniform).unwrap().functions.insert("color".into(), FunctionGraph::constant(value));
        let mut package = Package::new("Color function");
        package.graphs.push(graph);

        let cooked = cook(&package, 0, &Options::new(&no_res), &Mutex::new(Cache::default()));
        let result = &cooked.nodes[&uniform];

        assert_eq!(result.error, None);
        assert_eq!(result.parameters.get("color").and_then(Value::as_vec4), Some(expected));
    }
}

#[test]
fn levels_widens_scalar_endpoints_to_opaque_gray_and_normalizes_integer_controls() {
    use materialcraft_core::{FunctionGraph, OutputFormat};

    for (format, numerator, denominator, expected) in [
        (OutputFormat::F32, 1.0, 0.125, [8.0, 8.0, 8.0, 1.0]),
        (OutputFormat::U8, 1.0, 0.0, [1.0; 4]),
        (OutputFormat::U16, 1.0, -0.0, [0.0, 0.0, 0.0, 1.0]),
        (OutputFormat::U16, 0.0, 0.0, [0.0, 0.0, 0.0, 1.0]),
    ] {
        let mut function = FunctionGraph::default();
        let a = function.add_constant(Value::Float(numerator));
        let b = function.add_constant(Value::Float(denominator));
        function.output = Some(function.add("div", &[a, b], None));
        let mut graph = Graph::new("Channel control");
        let levels = graph.add_node("levels", [0.0; 2]);
        let node = graph.node_mut(levels).unwrap();
        node.base.format = format;
        node.functions.insert("in_high".into(), function);
        let mut package = Package::new("Channel control");
        package.graphs.push(graph);
        let cooked = cook(&package, 0, &Options::new(&no_res), &Mutex::new(Cache::default()));
        assert_eq!(cooked.nodes[&levels].error, None);
        assert_eq!(cooked.nodes[&levels].parameters["in_high"], Value::Float4(expected));
    }
}

fn instanced_seed_field() -> (Package, NodeId, NodeId) {
    let (mut parent, instance) = instancing("Parent", "Field");
    parent.seed = 19;
    parent.node_mut(instance).unwrap().base.seed = 3;
    let mut child = Graph::new("Field");
    child.size = parent.size;
    let noise = child.add_node("pixel_processor", [0.0; 2]);
    let mut function = materialcraft_core::FunctionGraph::default();
    let maximum = function.add_constant(Value::Float(1.0));
    function.output = Some(function.add("rand", &[maximum], None));
    child.node_mut(noise).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(noise, "output", output, "input").unwrap();
    let mut pkg = Package::new("Seed controls");
    pkg.graphs.extend([parent, child]);
    (pkg, instance, output)
}

#[test]
fn an_instanced_graph_resolves_its_own_seed_after_the_instance() {
    use materialcraft_core::ParameterInheritance::{Absolute, RelativeToInput, RelativeToParent};
    let (mut pkg, instance, output) = instanced_seed_field();
    let cache = Mutex::new(Cache::default());
    for (mode, offset, expected) in [(Absolute, 0, 0), (Absolute, 5, 5), (RelativeToParent, 5, 27), (RelativeToInput, -3, 19)] {
        pkg.graphs[1].seed = expected;
        let direct = cook(&pkg, 1, &Options::new(&no_res), &cache);
        pkg.graphs[1].seed = offset;
        pkg.graphs[1].seed_mode = mode;
        let nested = cook(&pkg, 0, &Options::new(&no_res), &cache);
        assert_eq!(nested.nodes[&instance].error, None);
        assert_eq!(nested.image(instance, 0).unwrap().data(), direct.image(output, 0).unwrap().data(), "{mode:?} {offset}");
    }
}

#[test]
fn graph_seed_functions_read_the_resolved_dimensions_of_the_child() {
    use materialcraft_core::{FunctionGraph, ParameterInheritance, ValueType};
    let (mut package, instance, output) = instanced_seed_field();
    package.graphs[1].seed_mode = ParameterInheritance::Absolute;
    let mut literal = package.clone();
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (name, axis, threshold, parent, absolute, expected_seed) in [
        ("$size", 0, 20.0, [5, 4], None, 5),
        ("$size", 1, 20.0, [5, 4], None, 0),
        ("$sizelog2", 1, 4.5, [4, 5], None, 5),
        ("$size", 0, 12.0, [5, 4], Some([3, 4]), 0),
        ("$sizelog2", 1, 3.5, [4, 5], Some([3, 4]), 5),
    ] {
        let mut function = FunctionGraph::input(name, ValueType::Float2);
        let selected = function.add("swizzle1", &[function.output.unwrap()], Some(Value::Int(axis)));
        let limit = function.add_constant(Value::Float(threshold));
        let predicate = function.add("gt", &[selected, limit], None);
        let yes = function.add_constant(Value::Int(5));
        let no = function.add_constant(Value::Int(0));
        function.output = Some(function.add("ifelse", &[predicate, yes, no], None));
        package.graphs[0].size = parent;
        package.graphs[1].absolute_output_size = absolute;
        package.graphs[1].seed_function = Some(function);
        literal.graphs[1].size = parent;
        literal.graphs[1].absolute_output_size = absolute;
        literal.graphs[1].seed = expected_seed;
        let actual = cook(&package, 0, &options, &cache);
        assert_eq!(actual.nodes[&instance].error, None);
        let reference = cook(&literal, 1, &options, &cache);
        assert_eq!(actual.image(instance, 0).unwrap().data(), reference.image(output, 0).unwrap().data(), "{name} {axis} {parent:?} {absolute:?}");
    }
}

#[test]
fn graph_seed_functions_use_the_child_argument_before_cooking_its_nodes() {
    use materialcraft_core::{ExposedParameter, FunctionGraph, ParameterInheritance, ValueType};
    let (mut package, instance, output) = instanced_seed_field();
    let input = ExposedParameter {
        id: "offset".into(),
        label: "Offset".into(),
        default: Value::Int(5),
        connectable: false,
        min: -20.0,
        max: 20.0,
        clamp: false,
        step: None,
        editor: None,
        description: String::new(),
        group: String::new(),
        options: Vec::new(),
    };
    package.graphs[1].exposed.push(input);
    let mut reference = package.clone();
    let cache = Mutex::new(Cache::default());
    let options = Options::new(&no_res);
    let function = FunctionGraph::input("offset", ValueType::Integer);
    let mut graph = serde_json::to_value(&package.graphs[1]).unwrap();
    graph["seedFunction"] = serde_json::to_value(&function).unwrap();
    package.graphs[1] = serde_json::from_value(graph).unwrap();
    for (mode, input, expected) in [
        (ParameterInheritance::Absolute, None, 5),
        (ParameterInheritance::RelativeToParent, None, 27),
        (ParameterInheritance::RelativeToInput, Some(-3), 19),
        (ParameterInheritance::Absolute, Some(-3), -3),
    ] {
        package.graphs[1].seed_mode = mode;
        if let Some(value) = input {
            package.graphs[0].node_mut(instance).unwrap().params.insert("offset".into(), Value::Int(value));
        }
        reference.graphs[1].seed = expected;
        let direct = cook(&reference, 1, &options, &cache);
        let nested = cook(&package, 0, &options, &cache);
        assert_eq!(nested.nodes[&instance].error, None);
        assert_eq!(nested.image(instance, 0).unwrap().data(), direct.image(output, 0).unwrap().data(), "{mode:?} {input:?}");
    }

    let mut wrong_type = FunctionGraph::default();
    wrong_type.output = Some(wrong_type.add_constant(Value::Text("invalid".into())));
    package.graphs[1].seed_function = Some(wrong_type);
    let invalid = cook(&package, 0, &options, &cache);
    assert!(invalid.nodes[&instance].error.as_deref().unwrap().contains("graph random seed"));
}

#[test]
fn nested_generators_keep_the_child_graph_output_format() {
    use materialcraft_core::OutputFormat;
    let (mut parent, instance) = instancing("Parent", "Field");
    let mut child = Graph::new("Field");
    child.size = parent.size;
    let processor = child.add_node("pixel_processor", [0.0; 2]);
    let mut function = materialcraft_core::FunctionGraph::default();
    let value = function.add_constant(Value::Float(0.347821));
    function.output = Some(value);
    child.node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(function)));
    let output = child.add_node("output", [100.0, 0.0]);
    child.connect(processor, "output", output, "input").unwrap();
    parent.format = OutputFormat::F32;
    let mut package = Package::new("Output precision");
    package.graphs.extend([parent, child]);
    let cache = Mutex::new(Cache::default());
    for caller in [OutputFormat::U8, OutputFormat::F32] {
        package.graphs[0].node_mut(instance).unwrap().base.format = caller;
        for child in OutputFormat::ALL {
            let expected = if child == OutputFormat::Inherit { caller } else { child };
            package.graphs[1].format = expected;
            let direct = cook(&package, 1, &Options::new(&no_res), &cache);
            package.graphs[1].format = child;
            let nested = cook(&package, 0, &Options::new(&no_res), &cache);
            assert_eq!(nested.nodes[&instance].error, None);
            assert_eq!(nested.nodes[&instance].output_base(0).unwrap().format, expected);
            assert_eq!(nested.image(instance, 0).unwrap().data(), direct.image(output, 0).unwrap().data(), "child {child:?} inside {caller:?}");
        }
    }
}

#[test]
fn nested_blurs_filter_the_original_caller_texture() {
    use materialcraft_core::OutputFormat;
    for filter in ["directional_blur", "blur"] {
        let (mut parent, instance) = instancing("Parent", "Filtered");
        let source = parent.add_node("input_grayscale", [0.0; 2]);
        parent.node_mut(source).unwrap().base.format = OutputFormat::F32;
        parent.connect(source, "output", instance, "input").unwrap();

        let mut child = Graph::new("Filtered");
        child.size = parent.size;
        let bridge = child.add_node("input_grayscale", [0.0; 2]);
        child.node_mut(bridge).unwrap().base.format = OutputFormat::U16;
        let add_filter = |graph: &mut Graph, input| {
            let id = graph.add_node(filter, [0.0; 2]);
            let node = graph.node_mut(id).unwrap();
            node.base.format = OutputFormat::U16;
            node.params.insert("intensity".into(), Value::Float(16.0));
            graph.connect(input, "output", id, "input").unwrap();
            id
        };
        let direct = add_filter(&mut parent, source);
        let nested = add_filter(&mut child, bridge);
        let output = child.add_node("output", [0.0; 2]);
        child.connect(nested, "output", output, "input").unwrap();
        let mut package = Package::new("Caller precision");
        package.graphs.extend([parent, child]);
        let mut options = Options::new(&no_res);
        options.inputs.insert("input".into(), Image::gray(16, 16, |x, y| ((97 * x + 31 * y + 13 * x * y) % 251) as f32 / 251.0));
        let cooked = cook(&package, 0, &options, &Mutex::new(Cache::default()));
        assert_eq!(cooked.nodes[&instance].error, None);
        let result = cooked.image(instance, 0).unwrap();
        let expected = cooked.image(direct, 0).unwrap();
        assert!(result.data().iter().zip(expected.data()).all(|(a, b)| a.to_bits() == b.to_bits()), "{filter}");
    }
}

#[test]
fn relative_graph_size_offsets_resolve_before_the_children_at_each_instance() {
    let (mut parent, instance) = instancing("Parent", "Field");
    parent.size = [5, 4];
    let mut child = Graph::new("Field");
    let fill = child.add_node("uniform", [0.0; 2]);
    let output = child.add_node("output", [1.0, 0.0]);
    child.connect(fill, "output", output, "input").unwrap();
    let mut pkg = Package::new("Graph dimensions");
    pkg.graphs.extend([parent, child]);
    let cache = Mutex::new(Cache::default());
    use materialcraft_core::GraphSizeOffset::{Input, Parent};
    for (offset, node_offset, expected) in
        [(Parent([-1, 1]), [0, 0], [16, 32]), (Input([1, -1]), [0, 0], [64, 8]), (Parent([-7, 0]), [3, 0], [2, 16]), (Input([0, 0]), [0, 0], [32, 16])]
    {
        pkg.graphs[1].relative_output_size = Some(offset);
        pkg.graphs[1].node_mut(fill).unwrap().base.size = node_offset;
        let result = cook(&pkg, 0, &Options::new(&no_res), &cache);
        assert_eq!(result.nodes[&instance].error, None);
        let image = result.image(instance, 0).unwrap();
        assert_eq!([image.width(), image.height()], expected);
    }
}

#[test]
fn computed_graph_output_sizes_read_instance_arguments_and_parent_axes() {
    use materialcraft_core::{ExposedParameter, FunctionGraph, GraphSizeOffset, ValueType};
    let (mut package, instance, _) = instanced_seed_field();
    package.graphs[0].size = [5, 4];
    let child = &mut package.graphs[1];
    let input = ExposedParameter {
        id: "axes".into(),
        label: "Axes".into(),
        default: Value::Int2([-1, 1]),
        connectable: false,
        min: -20.0,
        max: 20.0,
        clamp: false,
        step: None,
        editor: None,
        description: String::new(),
        group: String::new(),
        options: Vec::new(),
    };
    child.exposed.push(input);
    child.output_size_function = Some(FunctionGraph::input("axes", ValueType::Integer2));
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (absolute, mode, axes, expected) in [
        (None, Some(GraphSizeOffset::Parent([0, 0])), [-1, 1], [16, 32]),
        (None, Some(GraphSizeOffset::Input([0, 0])), [1, -1], [64, 8]),
        (Some([0, 0]), None, [3, 4], [8, 16]),
    ] {
        package.graphs[1].absolute_output_size = absolute;
        package.graphs[1].relative_output_size = mode;
        package.graphs[0].node_mut(instance).unwrap().params.insert("axes".into(), Value::Int2(axes));
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&instance].error, None);
        let image = result.image(instance, 0).unwrap();
        assert_eq!([image.width(), image.height()], expected);
    }
}

#[test]
fn computed_graph_output_sizes_test_both_axes_before_applying_their_offsets() {
    use materialcraft_core::{FunctionGraph, GraphSizeOffset, ValueType};
    let (mut package, instance, _) = instanced_seed_field();
    package.graphs[0].size = [5, 4];
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (name, axis, threshold, absolute, expected) in [
        ("$size", 0, 16.0, false, [64, 8]),
        ("$size", 1, 16.0, false, [16, 32]),
        ("$sizelog2", 0, 4.0, false, [64, 8]),
        ("$sizelog2", 1, 4.0, false, [16, 32]),
        ("$size", 0, 0.0, true, [8, 16]),
        ("$size", 0, -0.5, true, [16, 8]),
        ("$sizelog2", 1, 0.0, true, [8, 16]),
        ("$sizelog2", 1, -0.5, true, [16, 8]),
    ] {
        let mut function = FunctionGraph::input(name, ValueType::Float2);
        let channel = function.add("swizzle1", &[function.output.unwrap()], Some(Value::Int(axis)));
        let limit = function.add_constant(Value::Float(threshold));
        let pass = function.add("gt", &[channel, limit], None);
        let high = function.add_constant(Value::Int2(if absolute { [4, 3] } else { [1, -1] }));
        let low = function.add_constant(Value::Int2(if absolute { [3, 4] } else { [-1, 1] }));
        function.output = Some(function.add("ifelse", &[pass, high, low], None));
        let child = &mut package.graphs[1];
        child.absolute_output_size = absolute.then_some([0; 2]);
        child.relative_output_size = (!absolute).then_some(GraphSizeOffset::Parent([0; 2]));
        child.output_size_function = Some(function);
        let result = cook(&package, 0, &options, &cache);
        assert_eq!(result.nodes[&instance].error, None);
        let image = result.image(instance, 0).unwrap();
        assert_eq!([image.width(), image.height()], expected);
    }
    package.graphs[1].output_size_function = Some(FunctionGraph::constant(Value::Bool(true)));
    let invalid = cook(&package, 0, &options, &cache);
    assert!(invalid.nodes[&instance].error.as_deref().unwrap().contains("graph output size"));
}

#[test]
fn graph_seed_functions_use_the_allocated_size_below_one_texel() {
    use materialcraft_core::{FunctionGraph, GraphSizeOffset, ParameterInheritance, ValueType};
    let (mut package, instance, _) = instanced_seed_field();
    package.graphs[0].size = [4, 4];
    package.graphs[1].relative_output_size = Some(GraphSizeOffset::Parent([-6, 0]));
    package.graphs[1].seed_mode = ParameterInheritance::Absolute;
    let mut literal = package.clone();
    let options = Options::new(&no_res);
    let cache = Mutex::new(Cache::default());
    for (name, limit, expected) in [("$size", 0.5, 5), ("$size", 1.0, 0), ("$sizelog2", -0.5, 5), ("$sizelog2", 0.0, 0)] {
        let mut function = FunctionGraph::input(name, ValueType::Float2);
        let axis = function.add("swizzle1", &[function.output.unwrap()], Some(Value::Int(0)));
        let threshold = function.add_constant(Value::Float(limit));
        let pass = function.add("gt", &[axis, threshold], None);
        let yes = function.add_constant(Value::Int(5));
        let no = function.add_constant(Value::Int(0));
        function.output = Some(function.add("ifelse", &[pass, yes, no], None));
        package.graphs[1].seed_function = Some(function);
        literal.graphs[1].seed = expected;
        let result = cook(&package, 0, &options, &cache);
        let control = cook(&literal, 0, &options, &cache);
        let image = result.image(instance, 0).unwrap();
        assert_eq!([image.width(), image.height()], [1, 16]);
        assert_eq!(image.data(), control.image(instance, 0).unwrap().data());
    }
}
