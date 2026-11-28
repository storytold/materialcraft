use crate::{Cache, Cooked, NodeOutput, NodeResult, Options, identifier, now_ms};
use crate::{
    cache_key::{graph_tree_hash_with_registry, hash_of, image_key, variable_key},
    graph_instance::cook_instance,
    parameter::{ResolvedParameters, inspection_parameters, resolve_parameters},
    prepared_graph::PreparedGraph,
    value_execution::evaluate_value,
};
use craft_texture::{Image, Kind};
use materialcraft_core::{Node, OutputFormat, Package, Value};
use materialcraft_nodes::{CookContext, GraphVariables, find};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct BoundVariables<'a> {
    values: &'a BTreeMap<String, Value>,
    fixed: &'a [String],
    assigned_integers: &'a [String],
}

impl GraphVariables for BoundVariables<'_> {
    fn graph_values(&self) -> &BTreeMap<String, Value> {
        self.values
    }

    fn literal_inputs(&self) -> &[String] {
        self.fixed
    }

    fn assigned_integers(&self) -> &[String] {
        self.assigned_integers
    }
}

/// The magenta image a failed node shows
pub fn error_image(w: u32, h: u32) -> Image {
    Image::filled(w, h, Kind::Color, [1.0, 0.0, 1.0, 1.0])
}

/// Lock the cache, recovering from a poisoned lock (a cache entry is never half-written)
fn lock(cache: &Mutex<Cache>) -> std::sync::MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn cook_node(
    package: &Package,
    graph_index: usize,
    prepared: &PreparedGraph<'_>,
    node: &Node,
    cooked: &Cooked,
    graph_variables: &BTreeMap<String, Value>,
    options: &Options,
    input_image: Option<&Image>,
    cache: &Mutex<Cache>,
    previous: Option<&crate::PreviewSampling>,
    sampling: &mut crate::PreviewSampling,
) -> NodeResult {
    let (input_ports, output_ports) = prepared.ports(node.id);
    let definition = find(&node.def);

    // Gather inputs by port order, with their keys
    let mut inputs = Vec::new();
    let mut input_formats = Vec::new();
    let mut incoming = Vec::new();
    let mut input_keys = Vec::new();
    let mut base_level_only = [false; materialcraft_nodes::MAX_IMAGE_INPUTS + 1];
    let mut background_source = None;
    let mut variables = std::borrow::Cow::Borrowed(graph_variables);
    let mut fixed = std::borrow::Cow::Borrowed(options.literal_inputs.as_slice());
    let mut input_error = None;
    for port in input_ports {
        let connection = prepared.index.input_of(node.id, &port.id);
        let source = connection.and_then(|connection| {
            let result = cooked.nodes.get(&connection.from)?;
            let index = prepared.ports(connection.from).1.iter().position(|output| output.id == connection.from_port)?;
            Some((result.outputs.get(index)?, result.key ^ hash_of(&index), result.output_base(index)?))
        });
        input_keys.push(source.as_ref().map_or(0, |source| source.1));
        let output = source.map(|source| source.0);

        if let Some(expected) = port.value_type {
            let value = match output.as_ref() {
                Some(NodeOutput::Value(value)) if materialcraft_core::ValueType::of(value) == Some(expected) => value.clone(),
                Some(_) => {
                    input_error = Some(format!("input `{}` requires {}", port.id, expected.label()));
                    expected.default_value()
                }
                None => expected.default_value(),
            };

            variables.to_mut().insert(port.id.clone(), value);
            if fixed.contains(&port.id) {
                fixed.to_mut().retain(|name| name != &port.id);
            }
        } else {
            if output.as_ref().is_some_and(|output| output.as_value().is_some()) && !matches!(node.def.as_str(), "output" | "dot") {
                input_error = Some(format!("input `{}` requires an image", port.id));
            }

            let wired = if options.depth != 0 && matches!(node.def.as_str(), "pixel_processor" | "value_processor" | "fx_map" | "blur" | "directional_blur") {
                connection.and_then(|edge| prepared.index.node(edge.from)).and_then(|bridge| wired_texture(options, bridge))
            } else {
                None
            };
            if node.def == "fx_map"
                && let Some(level) = base_level_only.get_mut(incoming.len())
            {
                *level = wired.map_or(matches!(output, Some(NodeOutput::BaseLevel(_))), |(_, _, base_level)| base_level);
                if port.id == "background" {
                    background_source = if wired.is_some() {
                        connection.and_then(|edge| prepared.index.node(edge.from)).and_then(|bridge| options.input_bases.get(&identifier(bridge))).copied()
                    } else {
                        source.map(|(_, _, base)| base)
                    };
                }
            }
            let stored = source.and_then(|(output, _, base)| output.texture(base.format));
            let image = wired.map(|(image, format, _)| (image, format)).or(stored);
            inputs.push(image.map(|(image, _)| image.clone()));
            input_formats.push(image.map_or(OutputFormat::F32, |(_, format)| format));
        }

        incoming.push(source.map(|(output, _, base)| (output, base)));
    }

    let input_base = prepared.input_base(node.id, cooked).or_else(|| {
        if !matches!(node.def.as_str(), "input_color" | "input_grayscale") {
            return None;
        }
        let source = options.input_bases.get(&identifier(node))?;
        // Relative-to-input size follows this wire. The bridge still resolves its own storage precision.
        Some(crate::ResolvedBaseParameters {
            size: source.size,
            size_underflow: source.size_underflow,
            dynamic_size: source.dynamic_size,
            bound_size: source.bound_size,
            pixel_size: source.pixel_size,
            pixel_ratio: source.pixel_ratio,
            tiling: source.tiling,
            bound_image: source.bound_image,
            ..prepared.parent_base
        })
    });
    let mut assigned_integers = Vec::new();
    let mut pattern_writes = (node.def == "fx_map").then_some(&mut assigned_integers);
    let (mut base, base_values, base_error) =
        match crate::resolved_base_parameters::evaluate(&node.base, input_base, prepared.parent_base, &mut variables, &fixed, pattern_writes.as_deref_mut()) {
            Ok((base, values)) => (base, values, None),
            Err(error) => (crate::ResolvedBaseParameters::resolve(&node.base, input_base, prepared.parent_base), BTreeMap::new(), Some(error)),
        };

    if node.disabled
        && let Some(input) = input_base
    {
        base = input;
    }

    let (w, h) = crate::node_size::limit(base.size, options.max_size);
    let fail = |message: String, key: u64| NodeResult {
        base,
        base_values: base_values.clone(),
        parameters: BTreeMap::new(),
        outputs: Arc::new(vec![NodeOutput::Image(error_image(w, h)); output_ports.len().max(1)]),
        output_bases: None,
        error: Some(message),
        ms: 0.0,
        key,
        cached: false,
    };

    if let Some(error) = prepared.base_error.as_ref().or(base_error.as_ref()) {
        return fail(error.clone(), 0);
    }

    if let Some(error) = input_error {
        return fail(error, 0);
    }

    if !node.functions.is_empty()
        || matches!(
            node.def.as_str(),
            "pixel_processor"
                | "value_processor"
                | "fx_map"
                | "transformation"
                | "blend"
                | "levels"
                | "blur"
                | "directional_blur"
                | "directional_warp"
                | "warp"
                | "sharpen"
                | "distance"
                | "emboss"
        )
    {
        base.variables(variables.to_mut());
    }

    let seed = base.seed;
    let ResolvedParameters { values: mut parameters, instance_literals, dynamic_blend_opacity, bound_image_property } =
        match resolve_parameters(package, node, definition, &mut variables, base, &fixed, pattern_writes) {
            Ok(parameters) => parameters,
            Err(error) => return fail(error, 0),
        };
    let bound_image = if node.disabled {
        base.bound_image
    } else {
        base.bound_size
            || bound_image_property
            || dynamic_blend_opacity
            || (node.def == "fx_map" && base.bound_seed)
            || input_base.is_some_and(|source| source.bound_image)
            || incoming.iter().flatten().any(|(_, source)| source.bound_image)
    };
    let base = crate::ResolvedBaseParameters { bound_image, ..base };
    let background_tiling = background_source.filter(|source| source.dynamic_size == base.dynamic_size).map(|source| source.tiling);
    if matches!(node.def.as_str(), "blur" | "emboss") {
        variables.to_mut().insert("$pixelsize".into(), Value::Float2(base.pixel_size));
    }
    let variables = &variables;
    let bindings = BoundVariables { values: variables, fixed: &fixed, assigned_integers: &assigned_integers };

    let tile = base.tiling[0] && base.tiling[1];

    // Reuse the graph's decoded input while retaining its storage and binding metadata.
    let input_image = input_image.map(|image| {
        let name = identifier(node);
        if options.inputs.contains_key(&name) {
            let format = options.input_formats.get(&name).copied();
            return (image.clone(), format, options.bitmap_formats.get(&name).copied(), options.base_level_inputs.contains(&name));
        }
        let bitmap = if image.integer_sampling() {
            package
                .graphs
                .get(graph_index)
                .and_then(|graph| graph.input_images.get(&name))
                .and_then(|binding| package.resource(binding))
                .and_then(|resource| materialcraft_core::png_sample_format(resource.bytes.as_slice()))
        } else {
            None
        };
        (image.clone(), None, bitmap, false)
    });
    let wired = options.depth != 0 && input_image.as_ref().is_some_and(|(_, upstream, _, _)| upstream.is_some());
    let resource_hash = match parameters.get(crate::cache_key::resource_parameter(node)) {
        Some(Value::Text(name)) => package.resource(name).map_or(0, |r| r.bytes.content_hash()),
        _ => 0,
    };

    let registry = options.registry.unwrap_or(package);
    let subgraph_hash = if node.def == "graph_instance" {
        crate::graph_instance::resolve(registry, package, node).map_or(0, |(source, index, _)| graph_tree_hash_with_registry(registry, source, index))
    } else {
        0
    };

    let mut key = hash_of(&(
        &node.def,
        serde_json::to_string(&parameters).unwrap_or_default(),
        (dynamic_blend_opacity, bound_image, background_tiling),
        (
            w,
            h,
            seed,
            tile,
            node.disabled,
            base.size,
            base.size_underflow,
            (base.dynamic_size, base.bound_size),
            base.format,
            base.tiling,
            base.pixel_ratio,
            base.pixel_size.map(f32::to_bits),
        ),
        &input_keys,
        resource_hash,
        subgraph_hash,
        // Child outputs can exceed the instance size, so their cap is independent of w/h.
        (node.def == "graph_instance").then(|| (options.max_size, serde_json::to_string(&options.environment).unwrap_or_default())),
        input_image.as_ref().map(|(image, upstream, bitmap, base_level)| (image_key(image, cache), upstream, bitmap, wired, base_level)),
        (variable_key(node, &parameters, &bindings), &instance_literals),
    ));
    if options.previous_sampling.is_some() && input_image.is_some() {
        let name = identifier(node);
        if let Some(&upstream) = options.preview_input_keys.get(&name) {
            key = hash_of(&(key, upstream));
        } else if let Some(graph) = package.graphs.get(graph_index)
            && let Some(binding) = graph.input_images.get(&name)
        {
            key = hash_of(&(key, binding, graph.input_images_linear.contains(&name), graph.input_images_from_file.contains(&name)));
        }
    }
    let instance_input_key = key;
    if options.previous_sampling.is_some() && !node.disabled {
        if node.def == "graph_instance" {
            key = sampling.reuse_instance(previous, node.id, key);
        } else if node.def == "fx_map" && base_level_only.contains(&true) {
            key = sampling.stamp(previous, node.id, key, &input_keys, &mut base_level_only);
        }
    }
    if let Some(outputs) = lock(cache).get(key) {
        inspection_parameters(node, &mut parameters);
        let base = outputs.bases.as_deref().and_then(|ports| ports.first()).copied().unwrap_or(base);
        return NodeResult { base, base_values, parameters, outputs: outputs.values, output_bases: outputs.bases, error: None, ms: 0.0, key, cached: true };
    }

    let started_ms = now_ms();
    let mut output_bases: Option<Arc<[_]>> = None;
    let result: Result<Vec<NodeOutput>, String> = if node.disabled {
        let input = if let Some(primary) = definition.and_then(|definition| definition.bypass_input) {
            input_ports.iter().position(|port| port.id == primary).and_then(|index| incoming.get(index)).copied().flatten()
        } else {
            incoming.iter().find_map(|output| *output)
        };
        let output = input.map(|(output, _)| output.clone()).unwrap_or_else(|| NodeOutput::Image(Image::filled(w, h, Kind::Gray, [0.0; 4])));
        Ok(vec![output; output_ports.len().max(1)])
    } else if node.def == "input_value" {
        let value = variables.get(&identifier(node)).or_else(|| parameters.get("default")).cloned().unwrap_or(Value::Float(0.0));
        Ok(vec![NodeOutput::Value(value)])
    } else if node.def == "value_processor" {
        evaluate_value(&parameters, &bindings, seed, [w, h], &inputs, &input_formats, tile).map(|value| vec![NodeOutput::Value(value)])
    } else if matches!(node.def.as_str(), "output" | "dot")
        && let Some(Some((output, _))) = incoming.first()
        && (output.as_value().is_some()
            || matches!(output, NodeOutput::BaseLevel(stored) | NodeOutput::BoundBitmap { stored, .. } if (stored.width(), stored.height()) == (w, h)))
    {
        Ok(vec![(*output).clone()])
    } else if let Some((image, upstream, bitmap, inherited_base_level)) = input_image {
        let kind = if node.def == "input_grayscale" { Kind::Gray } else { Kind::Color };
        let format = upstream.unwrap_or(OutputFormat::F32);
        let source = bitmap.unwrap_or(format);
        let external = if wired { None } else { crate::external_image::resize(&image, (w, h), kind, source, base.format) };
        let tiling = if wired { base.tiling } else { [false; 2] };
        let output = external.unwrap_or_else(|| {
            let base_level = if wired {
                inherited_base_level && image.kind() == kind && (image.width(), image.height()) == (w, h) && format == base.format
            } else {
                crate::external_image::base_level_only(&image, (w, h), kind, source, base.format)
            };
            let resize = if wired { materialcraft_nodes::resize_connected_image } else { materialcraft_nodes::resize_bound_image };
            let image = resize(image, (w, h), kind, format, base.format, tiling);
            if base_level { NodeOutput::BaseLevel(image) } else { NodeOutput::Image(image) }
        });
        Ok(vec![output])
    } else if node.def == materialcraft_core::MISSING_NODE {
        let name = if node.label.is_empty() { "this node" } else { node.label.as_str() };

        Err(format!("unsupported node `{name}`"))
    } else if node.def == "graph_instance" {
        cook_instance(
            package,
            graph_index,
            node,
            input_ports,
            output_ports,
            &incoming,
            &input_keys,
            &parameters,
            &instance_literals,
            base,
            options,
            cache,
            previous.and_then(|p| p.child(node.id)),
        )
        .map(|instance| {
            if let Some((child, outputs)) = instance.sampling {
                key = hash_of(&(instance_input_key, outputs));
                sampling.record_instance(node.id, instance_input_key, key, child);
            }
            let (values, bases): (Vec<_>, Vec<_>) = instance.ports.into_iter().unzip();
            output_bases = Some(bases.into());
            values
        })
    } else if let Some(definition) = definition {
        if let (Some(tiling), Some(Some(image)), Some(&source)) = (background_tiling, inputs.first_mut(), input_formats.first())
            && let Some(filtered) = materialcraft_nodes::pattern::direct_background_sample(image, (w, h), source, tiling)
        {
            *image = filtered;
        }
        let store_inputs = match base.format {
            OutputFormat::F16 => matches!(node.def.as_str(), "blend" | "fx_map" | "levels" | "sharpen" | "blur"),
            OutputFormat::F32 => node.def == "blur",
            _ => false,
        };
        if store_inputs {
            // When exactly one size is parameter-controlled, these filters store normalized inputs at their working precision.
            // Images stamped by a pattern keep their sampling precision.
            for ((image, format), port) in inputs.iter_mut().zip(&mut input_formats).zip(input_ports) {
                if (node.def != "fx_map" || port.id == "background")
                    && matches!(format, OutputFormat::U8 | OutputFormat::U16 | OutputFormat::Inherit)
                    && let Some(image) = image
                    && (image.width(), image.height()) == (w, h)
                    && prepared.source_base(node.id, &port.id, cooked).is_some_and(|source| source.dynamic_size != base.dynamic_size)
                {
                    *image = crate::output_precision::apply(image.clone(), base.format);
                    if node.def == "blur" {
                        // Fractional blur taps use the working texture's sampler.
                        *format = base.format;
                    }
                }
            }
        }
        let context = CookContext {
            input_formats: &input_formats,
            format: base.format,
            w,
            h,
            seed,
            tile,
            inputs: &inputs,
            params: &parameters,
            resource: options.resource,
            vars: &bindings,
        };
        if node.def == "bitmap"
            && let Some(output) = crate::external_image::resize_bitmap(&context, package)
        {
            Ok(vec![output])
        } else {
            let result = if matches!(node.def.as_str(), "fx_map" | "pixel_processor") {
                let drawing = if node.def == "fx_map" {
                    materialcraft_nodes::pattern::cook_with_image_dependencies(&context, &base_level_only, bound_image)
                } else {
                    materialcraft_nodes::function::pixel_processor_with_dependencies(&context)
                };
                drawing.map(|(image, reads_input)| {
                    // A cached image carries dependencies read by its drawing program along with its pixels.
                    if reads_input && !bound_image {
                        output_bases = Some(Arc::from([crate::ResolvedBaseParameters { bound_image: true, ..base }]));
                    }
                    vec![image]
                })
            } else if node.def == "text"
                && !context.text("fontdata").is_empty()
                && let Some(font) = package.resource(context.text("fontdata"))
            {
                materialcraft_nodes::atomic::text_with_font(&context, font.bytes.as_slice())
            } else if dynamic_blend_opacity {
                materialcraft_nodes::atomic::blend_with_opacity(&context, true)
            } else {
                (definition.cook)(&context)
            };
            let selected = if node.def == "blend" && !dynamic_blend_opacity {
                materialcraft_nodes::atomic::blend_selected_input(&context).and_then(|slot| incoming.get(slot).copied().flatten())
            } else {
                None
            };
            result.map(|images| {
                images
                    .into_iter()
                    .map(|image| {
                        // Bridges retain their source pixels; filters store their declared precision
                        let image = if matches!(node.def.as_str(), "output" | "dot") { image } else { crate::output_precision::apply(image, base.format) };
                        // A Blend that selects a compatible texture retains its available sampling levels.
                        if let Some((NodeOutput::BaseLevel(source), source_base)) = selected
                            && source_base.format == base.format
                            && source.kind() == image.kind()
                            && (source.width(), source.height()) == (w, h)
                        {
                            return NodeOutput::BaseLevel(image);
                        }
                        NodeOutput::Image(image)
                    })
                    .collect()
            })
        }
    } else {
        Err(format!("unknown node `{}`", node.def))
    };

    // Cooked values serve parameter controls; authoring graphs stay in the document
    inspection_parameters(node, &mut parameters);
    match result {
        Ok(outputs) if outputs.len() == output_ports.len().max(1) || output_ports.is_empty() => {
            let outputs = Arc::new(outputs);
            lock(cache).put(key, outputs.clone(), output_bases.clone());
            let base = output_bases.as_deref().and_then(|ports| ports.first()).copied().unwrap_or(base);
            NodeResult { base, base_values, parameters, outputs, output_bases, error: None, ms: (now_ms() - started_ms) as f32, key, cached: false }
        }
        Ok(outputs) => fail(format!("`{}` made {} images for {} outputs", node.def, outputs.len(), output_ports.len()), key),
        Err(error) => fail(error, key),
    }
}

// Image reads inside an instance retain the caller texture's dimensions, depth and available mip levels.
fn wired_texture<'a>(options: &'a Options<'_>, bridge: &Node) -> Option<(&'a Image, OutputFormat, bool)> {
    let kind = match bridge.def.as_str() {
        "input_color" => Kind::Color,
        "input_grayscale" => Kind::Gray,
        _ => return None,
    };
    let name = identifier(bridge);
    let image = options.inputs.get(&name)?;
    (!bridge.disabled && image.kind() == kind).then_some((image, *options.input_formats.get(&name)?, options.base_level_inputs.contains(&name)))
}
