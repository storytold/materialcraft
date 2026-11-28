use crate::{Port, ports_with_connections};
use materialcraft_core::{Graph, GraphIndex, NodeId, Package};
use std::collections::BTreeMap;

/// Port definitions resolved once for a cook, alongside borrowed graph lookups
pub(crate) struct PreparedGraph<'a> {
    pub index: GraphIndex<'a>,
    pub order: Vec<NodeId>,
    pub parent_base: crate::resolved_base_parameters::ResolvedBaseParameters,
    pub base_error: Option<String>,
    ports: BTreeMap<NodeId, (Vec<Port>, Vec<Port>)>,
}

impl<'a> PreparedGraph<'a> {
    pub fn new(package: &Package, graph: &'a Graph, options: &crate::Options<'_>, variables: &BTreeMap<String, materialcraft_core::Value>) -> Self {
        let index = GraphIndex::new(graph);
        let needed = options.only.as_deref().map(|roots| index.required_nodes(roots));
        let order: Vec<_> = index.topo_order().into_iter().filter(|id| needed.as_ref().is_none_or(|needed| needed.contains(id))).collect();
        let ports = order
            .iter()
            .filter_map(|id| {
                let node = index.node(*id)?;
                Some((*id, ports_with_connections(package, node, index.incoming(*id).iter().copied())))
            })
            .collect();

        let mut parent_base = crate::resolved_base_parameters::ResolvedBaseParameters::graph(graph, options);
        let base_error =
            parent_base.evaluate_graph_size(graph, options, variables).and_then(|()| parent_base.evaluate_graph_seed(graph, options, variables)).err();
        Self { index, order, ports, parent_base, base_error }
    }

    pub fn input_base(&self, id: NodeId, cooked: &crate::Cooked) -> Option<crate::ResolvedBaseParameters> {
        let node = self.index.node(id)?;
        let definition = materialcraft_nodes::find(&node.def);
        let primary = definition.and_then(|definition| if node.disabled { definition.bypass_input } else { definition.primary_input });
        let mut base = self
            .ports(id)
            .0
            .iter()
            .filter(|port| port.kind != materialcraft_nodes::PortKind::Value && primary.is_none_or(|primary| port.id == primary))
            .find_map(|port| self.source_base(id, &port.id, cooked))?;

        // Only bypassed generators take their image dimensions from the input
        if !node.disabled && definition.is_some_and(|definition| definition.generator) {
            base.size = self.parent_base.size;
            base.size_underflow = self.parent_base.size_underflow;
            base.dynamic_size = self.parent_base.dynamic_size;
            base.bound_size = self.parent_base.bound_size;
        }

        Some(base)
    }

    pub fn source_base(&self, id: NodeId, port: &str, cooked: &crate::Cooked) -> Option<crate::ResolvedBaseParameters> {
        let edge = self.index.input_of(id, port)?;
        let index = self.ports(edge.from).1.iter().position(|output| output.id == edge.from_port)?;
        cooked.nodes.get(&edge.from)?.output_base(index)
    }

    pub fn ports(&self, id: NodeId) -> (&[Port], &[Port]) {
        self.ports.get(&id).map_or((&[], &[]), |(inputs, outputs)| (inputs.as_slice(), outputs.as_slice()))
    }
}
