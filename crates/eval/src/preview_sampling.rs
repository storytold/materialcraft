use crate::cache_key::hash_of;
use materialcraft_core::NodeId;
use std::{collections::BTreeMap, sync::Arc};

/// Sampling state from one accepted preview. A graph instance owns its child state.
#[derive(Clone, Debug, Default)]
pub struct PreviewSampling {
    pub(crate) context: ([u32; 2], u32, Option<u32>),
    pub(crate) caller_format: Option<materialcraft_core::OutputFormat>,
    stamps: BTreeMap<NodeId, Stamp>,
    instances: BTreeMap<NodeId, Instance>,
}

#[derive(Clone, Debug)]
struct Stamp {
    cook_key: u64,
    // Each port keeps the acquired texture identity and whether it used the full mip chain.
    inputs: BTreeMap<usize, (u64, bool)>,
}

#[derive(Clone, Debug)]
struct Instance {
    input_key: u64,
    output_key: u64,
    sampling: Arc<PreviewSampling>,
}

impl PreviewSampling {
    pub(crate) fn is_empty(&self) -> bool {
        self.stamps.is_empty() && self.instances.is_empty()
    }

    pub(crate) fn child(&self, id: NodeId) -> Option<&PreviewSampling> {
        self.instances.get(&id).map(|state| state.sampling.as_ref())
    }

    pub(crate) fn reuse_instance(&mut self, previous: Option<&Self>, id: NodeId, input_key: u64) -> u64 {
        if let Some(state) = previous.and_then(|p| p.instances.get(&id)).filter(|s| s.input_key == input_key) {
            self.instances.insert(id, state.clone());
            state.output_key
        } else {
            // Only an instance whose previous evaluation found no retained samplers can hit this key.
            hash_of(&(input_key, "preview without sampled bridges"))
        }
    }

    pub(crate) fn record_instance(&mut self, id: NodeId, input_key: u64, output_key: u64, sampling: Arc<Self>) {
        self.instances.insert(id, Instance { input_key, output_key, sampling });
    }

    pub(crate) fn stamp(&mut self, previous: Option<&Self>, id: NodeId, cook_key: u64, input_keys: &[u64], base_only: &mut [bool]) -> u64 {
        // A precision edit on the calling instance resamples retained textures even at a fixed child precision.
        let caller_changed = previous.is_some_and(|state| state.caller_format != self.caller_format);
        let previous = previous.and_then(|p| p.stamps.get(&id));
        let mut inputs = BTreeMap::new();
        let mut full = Vec::new();
        for (port, (base_only, &texture)) in base_only.iter_mut().zip(input_keys).enumerate() {
            if *base_only {
                let retained = previous.is_some_and(|stamp| {
                    stamp.inputs.get(&port).is_some_and(|&(old, full)| old == texture && (full || caller_changed || stamp.cook_key != cook_key))
                });
                inputs.insert(port, (texture, retained));
                if retained {
                    *base_only = false;
                    full.push(port);
                }
            }
        }
        self.stamps.insert(id, Stamp { cook_key, inputs });
        if full.is_empty() { cook_key } else { hash_of(&(cook_key, full)) }
    }
}
