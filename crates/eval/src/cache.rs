use crate::NodeOutput;
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_LIMIT_BYTES: usize = 512 << 20;
const MAX_ENTRIES: usize = 8192;

/// LRU cache of node outputs, bounded by bytes
pub struct Cache {
    entries: HashMap<u64, (Arc<Vec<NodeOutput>>, usize, u64)>,
    bytes: usize,
    cap: usize,
    tick: u64,
    pub(crate) input_hashes: HashMap<usize, crate::input_fingerprint::InputFingerprint>,
}

#[cfg(test)]
#[path = "cache.test.rs"]
mod tests;

impl Default for Cache {
    fn default() -> Self {
        // Active graph results are retained separately; history must not reserve a gigabyte
        Cache::new(DEFAULT_LIMIT_BYTES)
    }
}

impl Cache {
    pub fn new(cap_bytes: usize) -> Cache {
        Cache { entries: HashMap::new(), bytes: 0, cap: cap_bytes, tick: 0, input_hashes: HashMap::new() }
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.input_hashes.clear();
        self.bytes = 0;
    }

    pub(crate) fn get(&mut self, key: u64) -> Option<Arc<Vec<NodeOutput>>> {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        self.entries.get_mut(&key).map(|entry| {
            entry.2 = tick;
            entry.0.clone()
        })
    }

    pub(crate) fn put(&mut self, key: u64, outputs: Arc<Vec<NodeOutput>>) {
        let size = outputs.iter().fold(0_usize, |total, output| total.saturating_add(output.memory_bytes()));
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.1);
        }

        // Oversized results remain owned by their cooks
        // Empty results would permit unbounded zero-byte cache metadata
        if size == 0 || size > self.cap {
            return;
        }

        while self.bytes > self.cap - size || self.entries.len() >= MAX_ENTRIES {
            let Some((&oldest_key, _)) = self.entries.iter().min_by_key(|(_, entry)| entry.2) else { break };
            if let Some(entry) = self.entries.remove(&oldest_key) {
                self.bytes = self.bytes.saturating_sub(entry.1);
            }
        }

        self.tick = self.tick.saturating_add(1);
        self.entries.insert(key, (outputs, size, self.tick));
        self.bytes += size;
    }
}
