use crate::{Cache, Cooked, Options, now_ms};
use crate::{node_execution::cook_node, prepared_graph::PreparedGraph};
use craft_texture::Image;
use materialcraft_core::{Graph, Node, NodeId, Package, Value};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, atomic::Ordering},
};

pub fn cook(package: &Package, graph_index: usize, options: &Options, cache: &Mutex<Cache>) -> Cooked {
    let started_ms = now_ms();
    let mut cooked = Cooked::default();
    let Some(graph) = package.graphs.get(graph_index) else { return cooked };

    let mut previews = BTreeMap::new();
    let prepared = PreparedGraph::new(package, graph, options);

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

    let is_cancelled = || options.cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed));

    // Group the topological order into dependency-free wavefronts so independent branches
    // (multiple generators or output chains) cook concurrently across rayon workers.
    let waves = topological_waves(&prepared);

    for wave in waves {
        if is_cancelled() {
            cooked.cancelled = true;
            break;
        }

        if wave.len() == 1 {
            if let Some(&id) = wave.first()
                && let Some(node) = prepared.index.node(id)
            {
                let input = input_image(graph, node, options, &mut previews);
                if is_cancelled() {
                    cooked.cancelled = true;
                    break;
                }
                let result = cook_node(package, graph_index, &prepared, node, &cooked, &variables, options, input, cache);
                cooked.nodes.insert(id, result);
                if is_cancelled() {
                    cooked.cancelled = true;
                    break;
                }
            }
            continue;
        }

        // Pre-resolve any `input_*` preview images on the main thread before parallel dispatch.
        let mut wave_inputs: Vec<(NodeId, &Node, Option<Image>)> = Vec::with_capacity(wave.len());
        let mut cancelled_in_prep = false;
        for id in wave {
            if is_cancelled() {
                cancelled_in_prep = true;
                break;
            }
            if let Some(node) = prepared.index.node(id) {
                let img = input_image(graph, node, options, &mut previews).cloned();
                wave_inputs.push((id, node, img));
            }
        }
        if cancelled_in_prep {
            cooked.cancelled = true;
            break;
        }

        let results: Vec<(NodeId, crate::NodeResult)> = wave_inputs
            .par_iter()
            .filter_map(|(id, node, img)| {
                if is_cancelled() {
                    return None;
                }
                let result = cook_node(package, graph_index, &prepared, node, &cooked, &variables, options, img.as_ref(), cache);
                Some((*id, result))
            })
            .collect();
        let skipped = results.len() < wave_inputs.len();
        for (id, result) in results {
            cooked.nodes.insert(id, result);
        }
        if skipped || is_cancelled() {
            cooked.cancelled = true;
            break;
        }
    }

    cooked.ms = (now_ms() - started_ms) as f32;
    cooked
}

fn topological_waves(prepared: &PreparedGraph<'_>) -> Vec<Vec<NodeId>> {
    let mut depth_of: HashMap<NodeId, usize> = HashMap::with_capacity(prepared.order.len());
    let mut waves: Vec<Vec<NodeId>> = Vec::new();
    for &id in &prepared.order {
        let level = prepared.index.incoming(id).iter().filter_map(|wire| depth_of.get(&wire.from).map(|&d| d + 1)).max().unwrap_or(0);
        depth_of.insert(id, level);
        if waves.len() <= level {
            waves.resize_with(level + 1, Vec::new);
        }
        if let Some(wave) = waves.get_mut(level) {
            wave.push(id);
        }
    }
    waves
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
    let linear = graph.input_images_linear.contains(&identifier);

    // Nodes sharing an input also share its first lookup, including a missing preview
    previews.entry(identifier).or_insert_with(|| (options.resource)(&craft_texture::resource_key(resource, linear))).as_ref()
}
