use crate::NodeOutput;
use std::sync::Arc;

/// One node's cooked result
#[derive(Clone, Debug)]
pub struct NodeResult {
    pub base: crate::ResolvedBaseParameters,
    pub base_values: std::collections::BTreeMap<materialcraft_core::BaseParameter, materialcraft_core::Value>,
    /// Evaluated scalar and vector parameters, excluding authoring graphs and curves
    pub parameters: std::collections::BTreeMap<String, materialcraft_core::Value>,
    /// One image per output port (empty when the cook failed)
    pub outputs: Arc<Vec<NodeOutput>>,
    /// Instance ports carry the dimensions, format and sampling properties of their source.
    pub output_bases: Option<Arc<[crate::ResolvedBaseParameters]>>,
    pub error: Option<String>,
    /// Cook time in milliseconds (0 when served from the cache)
    pub ms: f32,
    pub key: u64,
    pub cached: bool,
}

impl NodeResult {
    pub fn output_base(&self, index: usize) -> Option<crate::ResolvedBaseParameters> {
        self.outputs.get(index)?;
        Some(self.output_bases.as_deref().and_then(|ports| ports.get(index)).copied().unwrap_or(self.base))
    }
}
