use crate::{Options, ResolvedBaseParameters, prepared_graph::PreparedGraph};
use materialcraft_core::{NodeId, Package};
use std::collections::BTreeMap;

/// Every node's resolved base parameters (uncapped size included) of graph `graph_index`, worked out
/// from the graph alone in topological order with no image or value cooks, so it is cheap enough
/// to call from the UI thread.
///
/// It follows the same inheritance as a cook: generators take the parent size, other nodes the
/// primary wired input's, and a disabled node passes its input through. Base parameters driven by
/// a function need the cooked values their function reads, so those are read as their literals.
pub fn resolve_bases(package: &Package, graph_index: usize, options: &Options<'_>) -> BTreeMap<NodeId, ResolvedBaseParameters> {
    let mut bases = BTreeMap::new();
    let Some(graph) = package.graphs.get(graph_index) else { return bases };
    let prepared = PreparedGraph::new(package, graph, options);
    for &id in &prepared.order {
        let Some(node) = prepared.index.node(id) else { continue };
        let input = prepared.input_base_from(id, |from| bases.get(&from).copied());
        let mut base = ResolvedBaseParameters::resolve(&node.base, input, prepared.parent_base);
        if node.disabled
            && let Some(input) = input
        {
            base = input;
        }

        bases.insert(id, base);
    }

    bases
}
