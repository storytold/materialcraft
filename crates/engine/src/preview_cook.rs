use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use materialcraft_core::Package;
use materialcraft_eval::{Cache, Cooked, Options, PreviewSampling};

/// One background result for the editor and the assigned material sources
#[derive(Default)]
pub struct PreviewCook {
    pub graph: usize,
    pub cooked: Cooked,
    pub materials: BTreeMap<usize, Cooked>,
}

impl PreviewCook {
    pub fn run(package: &Package, graph: usize, materials: &BTreeSet<usize>, options: &Options<'_>, cache: &Arc<Mutex<Cache>>) -> Self {
        Self::with_previous(package, graph, materials, options, cache, &BTreeMap::new())
    }

    pub fn with_previous(
        package: &Package,
        graph: usize,
        materials: &BTreeSet<usize>,
        options: &Options<'_>,
        cache: &Arc<Mutex<Cache>>,
        previous: &BTreeMap<usize, Arc<PreviewSampling>>,
    ) -> Self {
        let empty = PreviewSampling::default();
        let cook = |index| {
            let mut options = options.clone();
            options.previous_sampling = Some(previous.get(&index).map_or(&empty, Arc::as_ref));
            let mut cooked = materialcraft_eval::cook(package, index, &options, cache);
            if cooked.nodes.values().any(|node| node.error.is_some()) {
                cooked.preview_sampling = previous.get(&index).cloned();
            }
            cooked
        };
        let mut result = Self { graph, cooked: cook(graph), materials: BTreeMap::new() };
        for &index in materials {
            if result.cancelled() {
                break;
            }
            if index != graph && package.graphs.get(index).is_some() {
                result.materials.insert(index, cook(index));
            }
        }
        result
    }

    pub fn cancelled(&self) -> bool {
        self.cooked.cancelled || self.materials.values().any(|cooked| cooked.cancelled)
    }
}
