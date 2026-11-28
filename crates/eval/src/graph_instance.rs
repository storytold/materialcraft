use crate::{Cache, MAX_DEPTH, NodeOutput, Options, Port, cook, identifier};
use materialcraft_core::{Graph, Node, Package, Value};
use std::{collections::BTreeMap, sync::Mutex};

pub(crate) fn resolve<'a>(registry: &'a Package, current: &'a Package, node: &Node) -> Option<(&'a Package, usize, &'a Graph)> {
    let package = match &node.instance {
        Some(instance) if !instance.package.is_empty() => registry.dependencies.get(&instance.package)?.as_ref(),
        _ => current,
    };
    let name = node.instance.as_ref().map(|instance| instance.graph.as_str()).or_else(|| match node.params.get("graph") {
        Some(Value::Text(name)) => Some(name.as_str()),
        _ => None,
    })?;
    let (index, graph) = package.graph_by_name(name)?;
    Some((package, index, graph))
}

/// Whether graph `from` reaches `target`, without recursive stack growth
#[cfg(test)]
pub(crate) fn instances_reach(pkg: &Package, from: usize, target: usize) -> bool {
    let mut pending = vec![from];
    let mut seen = std::collections::HashSet::new();
    while let Some(index) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        let Some(graph) = pkg.graphs.get(index) else { continue };

        for (si, _) in graph.nodes.iter().filter(|n| n.def == "graph_instance").filter_map(|n| crate::instance_graph(pkg, n)) {
            if si == target {
                return true;
            }
            pending.push(si);
        }
    }
    false
}

pub(crate) struct CookedInstance {
    pub ports: Vec<(NodeOutput, crate::ResolvedBaseParameters)>,
    pub sampling: Option<(std::sync::Arc<crate::PreviewSampling>, u64)>,
}

pub(crate) fn cook_instance(
    package: &Package,
    graph_index: usize,
    node: &Node,
    input_ports: &[Port],
    output_ports: &[Port],
    inputs: &[Option<(&NodeOutput, crate::ResolvedBaseParameters)>],
    input_keys: &[u64],
    params: &BTreeMap<String, Value>,
    fixed: &[String],
    base: crate::resolved_base_parameters::ResolvedBaseParameters,
    options: &Options,
    cache: &Mutex<Cache>,
    previous: Option<&crate::PreviewSampling>,
) -> Result<CookedInstance, String> {
    if options.depth >= MAX_DEPTH {
        return Err("graph instances nest too deep (does a graph use itself?)".into());
    }

    let registry = options.registry.unwrap_or(package);
    let (source, source_graph_index, source_graph) = resolve(registry, package, node).ok_or("the instanced graph does not exist")?;
    let identity = std::ptr::from_ref(source_graph) as usize;
    if package.graphs.get(graph_index).is_some_and(|graph| std::ptr::eq(graph, source_graph)) || options.graph_stack.contains(&identity) {
        return Err(format!("graph `{}` uses itself through a graph instance", source_graph.name));
    }

    if source_graph.function.is_some() || !source_graph.nodes.iter().any(|node| node.def == "output") {
        return Err("the referenced graph must be a main graph with an output".into());
    }

    let root_resource = options.resource_root.unwrap_or(options.resource);
    let resource = |name: &str| match &node.instance {
        Some(instance) if !instance.package.is_empty() => {
            let (name, space) = materialcraft_nodes::BitmapColorSpace::split_resource_key(name);
            root_resource(&space.resource_key(&format!("{}\0{name}", instance.package)))
        }
        _ => (options.resource)(name),
    };
    let mut source_options = Options::new(&resource);
    let empty = crate::PreviewSampling::default();
    source_options.previous_sampling = options.previous_sampling.map(|_| previous.unwrap_or(&empty));
    source_options.environment = options.environment;
    source_options.parent_tiling = Some(base.tiling);
    source_options.parent_pixel_ratio = Some(base.pixel_ratio);
    source_options.parent_base = Some(base);
    source_options.registry = Some(registry);
    source_options.resource_root = Some(root_resource);
    source_options.max_size = options.max_size;
    source_options.cancel = options.cancel;
    source_options.depth = options.depth + 1;
    source_options.graph_stack = options.graph_stack.clone();
    source_options.graph_stack.push(identity);
    source_options.only = Some(source_graph.nodes.iter().filter(|node| node.def == "output").map(|node| node.id).collect());
    source_options.overrides = params.iter().filter(|(k, _)| source_graph.exposed.iter().any(|e| &e.id == *k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    for exposed in &source_graph.exposed {
        let name = &exposed.id;
        if fixed.contains(name) || (!params.contains_key(name) && source_graph.input_value(name).is_some_and(crate::parameter::supports_literal_argument)) {
            source_options.literal_inputs.push(name.clone());
        }
    }
    for ((port, input), &key) in input_ports.iter().zip(inputs).zip(input_keys) {
        // A stored argument drives its bridge even when the same port has a wire.
        if let Some(expected) = port.value_type
            && (node.params.contains_key(&port.id) || node.bindings.contains_key(&port.id) || node.functions.contains_key(&port.id))
            && let Some(value) = params.get(&port.id)
        {
            if materialcraft_core::ValueType::of(value) != Some(expected) {
                return Err(format!("input `{}` requires {}", port.id, expected.label()));
            }
            source_options.overrides.insert(port.id.clone(), value.clone());
            source_options.literal_inputs.retain(|name| name != &port.id);
            continue;
        }
        let Some((output, source_base)) = input else { continue };
        if let Some((image, texture_format)) = output.texture(source_base.format) {
            source_options.inputs.insert(port.id.clone(), image.clone());
            source_options.input_formats.insert(port.id.clone(), texture_format);
            source_options.input_bases.insert(port.id.clone(), *source_base);
            if matches!(output, NodeOutput::BaseLevel(_)) {
                source_options.base_level_inputs.insert(port.id.clone());
                if source_options.previous_sampling.is_some() {
                    source_options.preview_input_keys.insert(port.id.clone(), key);
                }
            }
        } else if let Some(value) = output.as_value() {
            source_options.overrides.insert(port.id.clone(), value.clone());
            source_options.literal_inputs.retain(|name| name != &port.id);
        }
    }
    let cooked = cook(source, source_graph_index, &source_options, cache);
    if cooked.cancelled {
        return Err("cancelled".into());
    }

    if let Some(error) = cooked.nodes.values().find_map(|result| result.error.as_deref()) {
        return Err(format!("{}: {error}", source_graph.name));
    }

    let mut keys = Vec::new();
    let ports = output_ports
        .iter()
        .map(|port| {
            let result = source_graph
                .nodes
                .iter()
                .find(|candidate| candidate.def == "output" && identifier(candidate) == port.id)
                .and_then(|node| cooked.nodes.get(&node.id))
                .ok_or_else(|| format!("the source output `{}` is missing", port.id))?;
            let output = result.outputs.first().ok_or_else(|| format!("the source output `{}` is empty", port.id))?;
            if cooked.preview_sampling.is_some() {
                keys.push(result.key);
            }
            // Images carry their source properties across the boundary. The caller owns the random seed.
            let port_base = crate::ResolvedBaseParameters { seed: base.seed, ..result.base };
            Ok((output.clone(), port_base))
        })
        .collect::<Result<_, String>>()?;
    let sampling = cooked.preview_sampling.map(|state| (state, crate::cache_key::hash_of(&keys)));
    Ok(CookedInstance { ports, sampling })
}
