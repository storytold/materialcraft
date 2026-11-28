use crate::{ResolvedBaseParameters, parameter_function::ParameterMode};
use materialcraft_core::{Node, OutputFormat, Package, Value, ValueType};
use materialcraft_nodes::function::compile_with_literal_inputs;
use materialcraft_nodes::{NodeDefinition, ParameterDescriptor, accepts_scalar_function, clamp_params, resolve_params};
use std::collections::BTreeMap;

pub(crate) struct ResolvedParameters {
    pub values: BTreeMap<String, Value>,
    pub instance_literals: Vec<String>,
    pub dynamic_blend_opacity: bool,
    pub bound_image_property: bool,
}

pub(crate) fn supports_literal_argument(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_) | Value::Float2(_) | Value::Float3(_) | Value::Float4(_))
}

// The color cook needs the operand's original width and whether its channels were folded.
fn uniform_color_operand(value: &Value, literal: bool) -> Option<Value> {
    match value {
        Value::Float(_) | Value::Float3(_) => Some(value.clone()),
        Value::Float4(rgba) | Value::Color(rgba) => Some(if literal { Value::Color(*rgba) } else { Value::Float4(*rgba) }),
        _ => None,
    }
}

pub(crate) fn resolve_parameters(
    package: &Package,
    node: &Node,
    definition: Option<&NodeDefinition>,
    variables: &mut std::borrow::Cow<'_, BTreeMap<String, Value>>,
    base: ResolvedBaseParameters,
    literal_inputs: &[String],
    mut assigned_integers: Option<&mut Vec<String>>,
) -> Result<ResolvedParameters, String> {
    let mut parameters = match definition {
        Some(definition) if node.def != "graph_instance" => resolve_params(definition, &node.params),
        _ => {
            let mut parameters = node.instance.as_ref().map(|instance| instance.defaults.clone()).unwrap_or_default();
            parameters.extend(node.params.clone());
            parameters
        }
    };

    for parameter in node.bindings.keys().chain(node.functions.keys()) {
        if !parameters.contains_key(parameter)
            && let Some(descriptor) = ParameterDescriptor::for_node(package, node, parameter)
        {
            parameters.insert(parameter.clone(), descriptor.default);
        }
    }

    let mut dynamic = std::collections::BTreeSet::new();
    let mut dynamic_blend_opacity = false;
    let mut bound_image_property = false;
    let mut bound_mip_level = false;
    let mut record_drawing_input = |name: &str| match (node.def.as_str(), name) {
        ("blend", "opacity") => dynamic_blend_opacity = true,
        ("transformation", "mip_level") => bound_mip_level = true,
        ("transformation", "matte") if base.tiling != [true; 2] => bound_image_property = true,
        ("uniform", "color")
        | ("blur" | "directional_blur" | "warp", "intensity")
        | ("directional_blur", "angle")
        | ("transformation", "offset" | "matrix")
        | ("grayscale_conversion", "weights" | "matte")
        | ("levels", "in_low" | "in_mid" | "in_high" | "out_low" | "out_high")
        | ("fx_map", "opacity" | "roughness" | "background" | "render_region" | "tiling_region") => bound_image_property = true,
        _ => {}
    };
    let mut uniform_color = None;
    for (parameter, exposed) in &node.bindings {
        if node.def == "uniform" && parameter == "color" {
            uniform_color = variables.get(exposed).and_then(|value| uniform_color_operand(value, false));
        }
        if node.def == "graph_instance" && !literal_inputs.contains(exposed) {
            dynamic.insert(parameter);
        }
        if let (Some(value), Some(current)) = (variables.get(exposed), parameters.get(parameter))
            && let Some(coerced) = current.coerce_json(&serde_json::to_value(value).unwrap_or_default())
        {
            if !literal_inputs.contains(exposed) && !node.functions.contains_key(parameter) {
                record_drawing_input(parameter);
            }
            parameters.insert(parameter.clone(), coerced);
        }
    }

    let mut gray_pattern_opacity = None;
    for (parameter, function) in &node.functions {
        let current = parameters.get(parameter).ok_or_else(|| format!("unknown function parameter `{parameter}`"))?;
        let expected = ValueType::of(current).ok_or_else(|| format!("parameter `{parameter}` cannot have a function"))?;
        let mode = match (node.def.as_str(), parameter.as_str(), base.format) {
            ("uniform", "color", _) => ParameterMode::UniformColor,
            ("levels", name, format) if accepts_scalar_function("levels", name) => {
                ParameterMode::Levels { normalized: name != "in_mid" && matches!(format, OutputFormat::U8 | OutputFormat::U16 | OutputFormat::Inherit) }
            }
            ("fx_map", "background", _) if matches!(current, Value::Float4(_)) => ParameterMode::Broadcast,
            ("fx_map", "opacity", _) if parameters.get("color_mode") == Some(&Value::Bool(false)) => ParameterMode::GrayPatternOpacity,
            _ => ParameterMode::Opaque,
        };
        let (value, literal) = compile_with_literal_inputs(function, base.seed, literal_inputs)
            .and_then(|program| {
                if program.reads_graph_parameter(variables) {
                    record_drawing_input(parameter);
                }
                crate::parameter_function::evaluate(&program, expected, variables.to_mut(), base.size, mode, Some(current), assigned_integers.as_deref_mut())
            })
            .map_err(|error| format!("function of `{parameter}`: {error}"))?;
        if matches!(mode, ParameterMode::UniformColor) {
            uniform_color = uniform_color_operand(&value, literal);
            continue;
        }
        if matches!(mode, ParameterMode::GrayPatternOpacity) {
            // Keep the result in floating point until blending. JSON and slider limits lose signed infinities.
            gray_pattern_opacity = Some(value);
            continue;
        }
        if node.def == "graph_instance" {
            if literal {
                dynamic.remove(parameter);
            } else {
                dynamic.insert(parameter);
            }
        }
        let value = if matches!((&mode, &value), (ParameterMode::Levels { .. }, Value::Float(_))) {
            value
        } else {
            current.coerce_json(&serde_json::to_value(value).unwrap_or_default()).ok_or_else(|| format!("invalid function value for `{parameter}`"))?
        };
        parameters.insert(parameter.clone(), value);
    }

    if let Some(color) = uniform_color {
        parameters.insert("color".into(), color);
    }
    // Catalog limits keep values from files and functions within safe cook bounds
    if let Some(definition) = definition
        && node.def != "graph_instance"
    {
        clamp_params(definition, &mut parameters);
    }
    if let Some(opacity) = gray_pattern_opacity {
        parameters.insert("opacity".into(), opacity);
    }
    // A computed mip mode can appear after the level in the property list.
    bound_image_property |= bound_mip_level && parameters.get("mip_mode").and_then(Value::as_i32) == Some(1);

    let fixed = if node.def == "graph_instance" {
        parameters.iter().filter(|(name, value)| supports_literal_argument(value) && !dynamic.contains(*name)).map(|(name, _)| name.clone()).collect()
    } else {
        Vec::new()
    };
    Ok(ResolvedParameters { values: parameters, instance_literals: fixed, dynamic_blend_opacity, bound_image_property })
}

pub(crate) fn inspection_parameters(node: &Node, parameters: &mut BTreeMap<String, Value>) {
    parameters.retain(|name, value| {
        if node.def == "uniform" && name == "color" {
            match value {
                Value::Float(level) => *value = Value::Color([*level, *level, *level, 1.0]),
                Value::Float3([r, g, b]) => *value = Value::Color([*r, *g, *b, 1.0]),
                Value::Float4(rgba) => *value = Value::Color(*rgba),
                _ => {}
            }
        }
        if let Value::Float(level) = value
            && node.def == "levels"
            && accepts_scalar_function("levels", name)
        {
            *value = Value::Float4([*level, *level, *level, 1.0]);
        }
        ValueType::of(value).is_some()
    });
}

#[cfg(test)]
mod tests {
    use crate::{Cache, Options, cook};
    use materialcraft_core::{FunctionGraph, Graph, Package, Value, ValueType};
    use std::sync::Mutex;

    #[test]
    fn uniform_gray_preserves_the_precision_of_its_color_source() {
        use materialcraft_core::OutputFormat;

        let mut graph = Graph::new("Uniform precision");
        graph.size = [2; 2];
        graph.format = OutputFormat::F32;
        let uniform = graph.add_node("uniform", [0.0; 2]);
        graph.node_mut(uniform).unwrap().params.insert("color_mode".into(), Value::Bool(false));
        let mut package = Package::new("Uniform precision");
        package.graphs.push(graph);
        let cache = Mutex::new(Cache::default());
        let mut options = Options::new(&|_| None);
        let scalar = Value::Float(f32::from_bits(0x3ed5_1632));
        let rgba = Value::Float4([1.0, f32::EPSILON, -1.0, 0.59375]);
        for (value, bound, expected) in [
            (rgba.clone(), false, 0x332a_aaab),
            (rgba.clone(), true, 0x3300_0000),
            (scalar, true, 0x3ed5_1632),
            (Value::Float3([0.125, 0.3125, 0.6875]), true, 0),
            (Value::Float4([-65536.0, 65536.0, 0.001953125, 0.5]), false, 0x3a2a_aaab),
            (rgba, false, 0x332a_aaab),
        ] {
            let function = if bound {
                let get = FunctionGraph::input("ink", ValueType::of(&value).unwrap());
                options.overrides.insert("ink".into(), value);
                get
            } else {
                FunctionGraph::constant(value)
            };
            package.graphs[0].node_mut(uniform).unwrap().functions.insert("color".into(), function);
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&uniform].error, None);
            assert_eq!(cooked.image(uniform, 0).unwrap().get_gray(0, 0, false).to_bits(), expected);
        }
    }

    #[test]
    fn grayscale_uniform_distinguishes_image_size_from_an_exposed_color_read() {
        use materialcraft_core::OutputFormat;

        let mut graph = Graph::new("Uniform sources");
        graph.size = [2; 2];
        graph.format = OutputFormat::F32;
        let uniform = graph.add_node("uniform", [0.0; 2]);
        graph.node_mut(uniform).unwrap().params.insert("color_mode".into(), Value::Bool(false));
        let mut package = Package::new("Uniform sources");
        package.graphs.push(graph);
        let cache = Mutex::new(Cache::default());
        let mut options = Options::new(&|_| None);
        options.overrides.insert("ink".into(), Value::Float4([0.25; 4]));

        for (variable, ty, expected) in [
            ("$size", ValueType::Float2, 0x332a_aaab),
            ("$pixelratio", ValueType::Integer, 0x332a_aaab),
            ("$uvtile", ValueType::Integer2, 0x332a_aaab),
            ("$physicalsize", ValueType::Float3, 0x332a_aaab),
            ("unbound_color", ValueType::Float, 0x332a_aaab),
            ("$time", ValueType::Float, 0x3300_0000),
            ("$normalformat", ValueType::Integer, 0x3300_0000),
            ("ink", ValueType::Float4, 0x3300_0000),
        ] {
            let mut function = FunctionGraph::input(variable, ty);
            let read = function.output.unwrap();
            let rgba = function.add_constant(Value::Float4([1.0, f32::EPSILON, -1.0, 0.59375]));
            function.output = Some(function.add("sequence", &[read, rgba], None));
            package.graphs[0].node_mut(uniform).unwrap().functions.insert("color".into(), function);
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&uniform].error, None);
            assert_eq!(cooked.image(uniform, 0).unwrap().get_gray(0, 0, false).to_bits(), expected);
        }
    }

    #[test]
    fn uniform_color_keeps_hdr_channels_and_distinguishes_an_explicit_alpha() {
        use materialcraft_core::OutputFormat;

        let mut graph = Graph::new("Uniform channels");
        graph.size = [2; 2];
        graph.format = OutputFormat::F32;
        let uniform = graph.add_node("uniform", [0.0; 2]);
        let mut package = Package::new("Uniform channels");
        package.graphs.push(graph);
        let cache = Mutex::new(Cache::default());
        let mut options = Options::new(&|_| None);
        let rgb = [-65536.0, 65536.0, 0.001953125];
        options.overrides.insert("ink".into(), Value::Float3(rgb));
        let direct = FunctionGraph::input("ink", ValueType::Float3);
        let mut with_alpha = direct.clone();
        let color = with_alpha.output.unwrap();
        let alpha = with_alpha.add_constant(Value::Float(0.59375));
        with_alpha.output = Some(with_alpha.add("vector4", &[color, alpha], None));
        for (function, expected) in [(direct, [0.0, 0.0, 0.0, 1.0]), (with_alpha, [rgb[0], rgb[1], rgb[2], 0.59375])] {
            package.graphs[0].node_mut(uniform).unwrap().functions.insert("color".into(), function);
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&uniform].error, None);
            assert_eq!(cooked.image(uniform, 0).unwrap().get(0, 0, false), expected);
        }
    }

    #[test]
    fn gray_levels_keep_scalar_endpoints_distinct_from_equal_rgb_controls() {
        use craft_texture::Image;
        use materialcraft_core::OutputFormat;

        let mut graph = Graph::new("Endpoint precision");
        graph.size = [2, 2];
        graph.format = OutputFormat::F32;
        let source = graph.add_node("input_grayscale", [0.0; 2]);
        let levels = graph.add_node("levels", [0.0; 2]);
        graph.connect(source, "output", levels, "input").unwrap();
        let mut package = Package::new("Endpoint precision");
        package.graphs.push(graph);
        let mut options = Options::new(&|_| None);
        options.inputs.insert("input".into(), Image::gray(4, 4, |_, _| 1.0));
        let cache = Mutex::new(Cache::default());

        let scalar = f32::from_bits(0x3ec0c0c2);
        let rgba = [scalar, scalar, scalar, 1.0];
        let vector_result = f32::from_bits(0x3ec0c0c3);
        for (value, expected) in [(Value::Float4(rgba), vector_result), (Value::Float(scalar), scalar), (Value::Float4(rgba), vector_result)] {
            package.graphs[0].node_mut(levels).unwrap().functions.insert("out_high".into(), FunctionGraph::constant(value));
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&levels].parameters["out_high"], Value::Float4(rgba));
            assert_eq!(cooked.image(levels, 0).unwrap().get_gray(0, 0, false).to_bits(), expected.to_bits());
        }
    }

    #[test]
    fn missing_direct_level_link_keeps_the_property_value() {
        let mut graph = Graph::new("Matte");
        graph.size = [2; 2];
        let levels = graph.add_node("levels", [0.0; 2]);
        let mut package = Package::new("Links");
        package.graphs.push(graph);
        let cache = Mutex::new(Cache::default());
        let reader = FunctionGraph::input("%amount", ValueType::Float);
        let mut disconnected = reader.clone();
        disconnected.add_constant(Value::Float(0.75));
        let mut computed = reader.clone();
        let value = computed.output.unwrap();
        let offset = computed.add_constant(Value::Float(0.1875));
        computed.output = Some(computed.add("add", &[value, offset], None));

        for (function, bound, expected) in [
            (reader.clone(), None, [1.0; 4]),
            (disconnected, None, [1.0; 4]),
            (reader, Some(0.0), [0.0, 0.0, 0.0, 1.0]),
            (computed, None, [0.1875, 0.1875, 0.1875, 1.0]),
        ] {
            package.graphs[0].node_mut(levels).unwrap().functions.insert("out_high".into(), function);
            let mut options = Options::new(&|_| None);
            if let Some(level) = bound {
                options.overrides.insert("amount".into(), Value::Float(level));
            }
            let cooked = cook(&package, 0, &options, &cache);
            assert_eq!(cooked.nodes[&levels].error, None);
            assert_eq!(cooked.nodes[&levels].parameters.get("out_high"), Some(&Value::Float4(expected)));
        }
    }
}
