use crate::{Cache, Cooked, Options, now_ms};
use crate::{node_execution::cook_node, prepared_graph::PreparedGraph};
use craft_texture::Image;
use materialcraft_core::{Graph, Node, Package, Value};
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::Ordering},
};

/// Resolve a document graph's output dimensions without allocating or cooking its textures.
pub fn graph_output_size(graph: &materialcraft_core::Graph, environment: materialcraft_core::EvaluationEnvironment) -> Result<[u32; 2], String> {
    if graph.output_size_function.is_none() {
        return Ok(graph.size_px());
    }
    let mut options = Options::new(&|_| None);
    options.environment = environment;
    let mut base = crate::ResolvedBaseParameters::graph(graph, &options);
    base.evaluate_graph_size(graph, &options, &graph_variables(graph, &options))?;
    Ok(base.size)
}

pub fn cook(package: &Package, graph_index: usize, options: &Options, cache: &Mutex<Cache>) -> Cooked {
    let started_ms = now_ms();
    let mut cooked = Cooked::default();
    let Some(graph) = package.graphs.get(graph_index) else { return cooked };

    let mut previews = BTreeMap::new();
    let variables = graph_variables(graph, options);

    let prepared = PreparedGraph::new(package, graph, options, &variables);
    let mut sampling = crate::PreviewSampling::default();
    sampling.context = (prepared.parent_base.size, prepared.parent_base.seed, options.max_size);
    sampling.caller_format = options.parent_base.map(|base| base.format);
    let previous = options.previous_sampling.filter(|p| p.context == sampling.context);
    for &id in &prepared.order {
        if options.cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            cooked.cancelled = true;
            break;
        }

        let Some(node) = prepared.index.node(id) else { continue };

        let input = input_image(graph, node, options, &mut previews);
        let result = cook_node(package, graph_index, &prepared, node, &cooked, &variables, options, input, cache, previous, &mut sampling);
        cooked.nodes.insert(id, result);
    }

    cooked.ms = (now_ms() - started_ms) as f32;
    if !sampling.is_empty() && !cooked.cancelled && cooked.nodes.values().all(|result| result.error.is_none()) {
        cooked.preview_sampling = Some(std::sync::Arc::new(sampling));
    }
    cooked
}

fn graph_variables(graph: &materialcraft_core::Graph, options: &Options) -> BTreeMap<String, Value> {
    // Exposed parameter values: the graph's own, then the caller's overrides
    let mut variables: BTreeMap<String, Value> =
        graph.exposed.iter().filter_map(|exposed| graph.input_value(&exposed.id).map(|value| (exposed.id.clone(), value.clone()))).collect();
    for (name, value) in &options.overrides {
        variables.insert(name.clone(), value.clone());
    }
    variables.insert("$time".into(), Value::Float(options.environment.time_seconds));
    variables.insert("$uvtile".into(), Value::Int2(options.environment.uv_tile));
    variables.insert("$normalformat".into(), Value::Int(options.environment.normal_format));
    variables.insert("$physicalsize".into(), Value::Float3(graph.physical_size));
    variables
}

fn input_image<'a>(graph: &Graph, node: &Node, options: &'a Options<'_>, previews: &'a mut BTreeMap<String, Option<Image>>) -> Option<&'a Image> {
    if !node.def.starts_with("input_") {
        return None;
    }

    let identifier = crate::identifier(node);
    if let Some(image) = options.inputs.get(&identifier) {
        return Some(image);
    }

    let resource = graph.input_images.get(&identifier)?;
    let space = if graph.input_images_from_file.contains(&identifier) {
        materialcraft_nodes::BitmapColorSpace::FromFile
    } else if graph.input_images_linear.contains(&identifier) {
        materialcraft_nodes::BitmapColorSpace::Linear
    } else {
        materialcraft_nodes::BitmapColorSpace::Srgb
    };

    // Nodes sharing an input also share its first lookup, including a missing preview
    previews.entry(identifier).or_insert_with(|| (options.resource)(&space.resource_key(resource))).as_ref()
}
