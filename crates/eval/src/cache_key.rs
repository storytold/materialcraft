use crate::{Cache, identifier};
use craft_texture::Image;
use materialcraft_core::{Node, Package, Value};
use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

const MAX_INPUT_FINGERPRINTS: usize = 128;

pub(crate) fn image_key(image: &Image, cache: &Mutex<Cache>) -> u64 {
    let buffer_id = Arc::as_ptr(image.shared_data()) as usize;
    let shape = (image.width(), image.height(), image.channels());
    if let Some(key) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .input_hashes
        .get(&buffer_id)
        .and_then(|saved| saved.buffer.upgrade().filter(|data| Arc::ptr_eq(data, image.shared_data()) && saved.shape == shape).map(|_| saved.key))
    {
        return key ^ hash_of(&image.integer_sampling());
    }

    let mut hasher = DefaultHasher::new();
    (shape, image.data().len()).hash(&mut hasher);
    for value in image.data() {
        value.to_bits().hash(&mut hasher);
    }

    let key = hasher.finish();
    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.input_hashes.len() >= MAX_INPUT_FINGERPRINTS {
        cache.input_hashes.retain(|_, saved| saved.buffer.strong_count() > 0);
        if cache.input_hashes.len() >= MAX_INPUT_FINGERPRINTS {
            cache.input_hashes.clear();
        }
    }

    cache.input_hashes.insert(buffer_id, crate::input_fingerprint::InputFingerprint { buffer: Arc::downgrade(image.shared_data()), shape, key });
    key ^ hash_of(&image.integer_sampling())
}

pub(crate) fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
pub(crate) fn graph_tree_hash(package: &Package, graph_index: usize) -> u64 {
    graph_tree_hash_with_registry(package, package, graph_index)
}

pub(crate) fn resource_parameter(node: &Node) -> &'static str {
    if node.def == "text" { "fontdata" } else { "resource" }
}

pub(crate) fn graph_tree_hash_with_registry(registry: &Package, package: &Package, graph_index: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut pending = vec![(package, graph_index)];
    let mut seen = HashSet::new();
    while let Some((package, index)) = pending.pop() {
        if !seen.insert((std::ptr::from_ref(package) as usize, index)) {
            continue;
        }

        let Some(graph) = package.graphs.get(index) else { continue };

        index.hash(&mut hasher);
        serde_json::to_string(graph).unwrap_or_default().hash(&mut hasher);
        for (id, resource_name) in &graph.input_images {
            package.resource(resource_name).map(|resource| resource.bytes.content_hash()).hash(&mut hasher);
            id.hash(&mut hasher);
            graph.input_images_linear.contains(id).hash(&mut hasher);
        }

        for instance in graph.functions().flat_map(|function| &function.nodes).filter_map(|node| node.instance.as_ref()) {
            let source = if instance.package.is_empty() { Some(package) } else { registry.dependencies.get(&instance.package).map(AsRef::as_ref) };
            if let Some(source) = source
                && let Some((index, _)) = source.graph_by_name(&instance.graph)
            {
                pending.push((source, index));
            }
        }

        for node in &graph.nodes {
            let parameter = resource_parameter(node);
            if node.bindings.contains_key(parameter) || node.functions.contains_key(parameter) {
                // The caller may select another package resource when this instance runs.
                for resource in &package.resources {
                    resource.name.hash(&mut hasher);
                    resource.bytes.content_hash().hash(&mut hasher);
                }
            } else if let Some(Value::Text(name)) = node.params.get(parameter) {
                package.resource(name).map(|resource| resource.bytes.content_hash()).hash(&mut hasher);
            }
            if node.def == "graph_instance"
                && let Some((source, index, _)) = crate::graph_instance::resolve(registry, package, node)
            {
                pending.push((source, index));
            }
        }
    }

    hasher.finish()
}

pub(crate) fn variable_key(node: &Node, parameters: &BTreeMap<String, Value>, variables: &dyn materialcraft_nodes::GraphVariables) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut names = std::collections::BTreeSet::new();
    let mut pending: Vec<&Value> = parameters.values().collect();
    while let Some(value) = pending.pop() {
        match value {
            // JSON represents all non-finite numbers as null. Their signs change pattern blending.
            Value::Float(v) if !v.is_finite() => v.to_bits().hash(&mut hasher),
            Value::Function(graph) => {
                let mut pending = vec![graph.as_ref()];
                while let Some(graph) = pending.pop() {
                    serde_json::to_string(graph).unwrap_or_default().hash(&mut hasher);
                    for function_node in &graph.nodes {
                        // Resolved call defaults are omitted from graph serialization
                        if let Some(instance) = &function_node.instance {
                            serde_json::to_string(&instance.defaults).unwrap_or_default().hash(&mut hasher);
                        }

                        match &function_node.source {
                            Some(Ok(source)) => pending.push(source.as_ref()),
                            Some(Err(error)) => error.hash(&mut hasher),
                            None => {}
                        }
                        if let Some(Value::Text(name)) = &function_node.value {
                            if function_node.op.starts_with("get_") {
                                names.insert(name.strip_prefix('%').unwrap_or(name));
                            } else if function_node.op == "isdefined" {
                                names.insert(name.as_str());
                            }
                        }
                    }
                }
            }
            Value::PatternGraph(graph) => {
                pending.extend(graph.nodes.iter().flat_map(|node| node.parameters.values()));
            }
            _ => {}
        }
    }

    let input_identifier = identifier(node);
    if node.def == "input_value" {
        names.insert(&input_identifier);
    }

    for name in names {
        name.hash(&mut hasher);
        variables.get(name).map(|value| serde_json::to_string(value).unwrap_or_default()).hash(&mut hasher);
        variables.literal_inputs().iter().any(|input| input == name).hash(&mut hasher);
        if node.def == "fx_map" {
            variables.assigned_integers().iter().any(|input| input == name).hash(&mut hasher);
        }
    }

    hasher.finish()
}
