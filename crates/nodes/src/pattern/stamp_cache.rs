use super::pattern_stamp::PatternStamp;
use crate::function::Program;
use materialcraft_core::{PatternNode, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A terminal draw can reuse its properties across positions when its programs
/// only depend on the traversal seed, depth and inputs fixed for this cook.
pub(super) fn reusable(program: &Program, assigned: &BTreeSet<&str>) -> bool {
    (0..program.instructions.len()).all(|index| reusable_at(program, index, assigned))
}

fn reusable_at(program: &Program, index: usize, assigned: &BTreeSet<&str>) -> bool {
    let Some(op) = program.instructions.get(index) else { return false };
    let id = op.definition.id;
    if matches!(id, "set" | "while" | "samplecol" | "samplelum") {
        return false;
    }
    if (id.starts_with("get_") || id == "isdefined")
        && let Some(Value::Text(name)) = &op.value
        && (matches!(name.as_str(), "$pos" | "$number") || assigned.contains(name.as_str()))
    {
        return false;
    }
    op.call.as_ref().is_none_or(|call| reusable(&call.program, assigned))
}

pub(super) fn reusable_inputs(program: &Program, outputs: &[usize], assigned: &BTreeSet<&str>) -> bool {
    let mut pending = outputs.to_vec();
    let mut seen = vec![false; program.instructions.len()];
    while let Some(index) = pending.pop() {
        let Some(visited) = seen.get_mut(index) else { return false };
        if *visited {
            continue;
        }
        *visited = true;
        if !reusable_at(program, index, assigned) {
            return false;
        }
        pending.extend(program.instructions[index].inputs.iter().copied());
    }
    true
}

pub(super) fn independent_of_depth(program: &Program) -> bool {
    program.instructions.iter().all(|op| {
        !matches!(&op.value, Some(Value::Text(name)) if matches!(name.as_str(), "$depth" | "$depthpow2"))
            && op.call.as_ref().is_none_or(|call| independent_of_depth(&call.program))
    })
}

/// Identical authoring data can supply a branch offset and tint at several drawing depths.
#[derive(Default)]
pub(super) struct BranchGroups(HashMap<Vec<u8>, (u32, BTreeMap<String, Value>)>);

impl BranchGroups {
    pub fn storage(&mut self, node: &PatternNode) -> Option<Storage> {
        let size = match node.parameters.get("patternsize") {
            Some(Value::Float2(size)) => *size,
            None => [1.0; 2],
            _ => return None,
        };
        let mut parameters = node.parameters.clone();
        // An ordinary save assigns fresh IDs to each expression. Only its result and wiring use them.
        for value in parameters.values_mut() {
            let Value::Function(graph) = value else { continue };
            let ids: BTreeMap<_, _> = graph.nodes.iter().map(|n| n.id).zip(0_u32..).collect();
            let output = graph.output.as_mut()?;
            *output = *ids.get(output)?;
            for (id, n) in (0_u32..).zip(&mut graph.nodes) {
                n.id = id;
                for input in n.inputs.iter_mut().flatten() {
                    *input = *ids.get(input)?;
                }
            }
        }
        let key = serde_json::to_vec(&parameters).ok()?;
        // Serialization distinguishes signed zeros. Compare resolved calls and nonfinite values as well.
        let group = match self.0.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) if entry.get().1 == parameters => entry.get().0,
            entry => {
                entry.or_insert((node.id, parameters));
                node.id
            }
        };
        Some(Storage::BranchTint { group, size })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Key {
    pub node: u32,
    pub depth: u32,
    pub seed: u32,
}

#[derive(Clone)]
pub(super) struct Properties {
    pub branch_offset: [f32; 2],
    /// The center holds the frame offset until the two translations have rounded separately.
    pub stamp: PatternStamp,
}

#[derive(Clone, Copy)]
pub(super) enum Storage {
    Full,
    FixedImage,
    Placement,
    Geometry { terminal: bool },
    BranchTint { group: u32, size: [f32; 2] },
}

// Each node and depth has immutable properties. A sparse seed page points
// to its changing fields, which occupy contiguous blocks across all depths.
#[derive(Default)]
struct PackedStamps<const FIELDS: usize> {
    templates: HashMap<(u32, u32), Properties>,
    pages: HashMap<(u32, u32, u16), Box<[u32]>>,
    values: Vec<Vec<[f32; FIELDS]>>,
    bytes: usize,
}

impl<const FIELDS: usize> PackedStamps<FIELDS> {
    const PAGE_SIZE: usize = 256;
    const BLOCK_SIZE: usize = 4_096;

    fn address(key: Key) -> ((u32, u32, u16), usize) {
        ((key.node, key.depth, key.seed as u16), (key.seed >> 16) as usize)
    }

    fn get(&self, key: Key) -> Option<(Properties, [f32; FIELDS])> {
        let (page, index) = Self::address(key);
        let at = self.pages.get(&page)?.get(index)?.checked_sub(1)? as usize;
        let fields = *self.values.get(at / Self::BLOCK_SIZE)?.get(at % Self::BLOCK_SIZE)?;
        Some((self.templates.get(&(key.node, key.depth))?.clone(), fields))
    }

    fn insert(&mut self, key: Key, properties: &Properties, fields: [f32; FIELDS], limit: usize) -> bool {
        // Child streams have 24-bit seeds. Wider explicit seeds use the small cache.
        if key.seed >> 24 != 0 {
            return false;
        }
        let (page, index) = Self::address(key);
        let new_block = self.values.last().is_none_or(|block| block.len() == Self::BLOCK_SIZE);
        let growth = usize::from(new_block) * Self::BLOCK_SIZE * size_of::<[f32; FIELDS]>()
            + usize::from(!self.pages.contains_key(&page)) * Self::PAGE_SIZE * size_of::<u32>();
        // Bound both indices and floating-point storage independently of the traversal length.
        if self.bytes.saturating_add(growth) > limit {
            return false;
        }
        let entries = self.pages.entry(page).or_insert_with(|| vec![0; Self::PAGE_SIZE].into_boxed_slice());
        let Some(entry) = entries.get_mut(index) else { return false };
        if *entry != 0 {
            return true;
        }
        self.bytes += growth;
        if new_block {
            self.values.push(Vec::with_capacity(Self::BLOCK_SIZE));
        }
        let base = self.values.len().saturating_sub(1) * Self::BLOCK_SIZE;
        let Some(block) = self.values.last_mut() else { return false };
        let Ok(at) = u32::try_from(base + block.len() + 1) else { return false };
        block.push(fields);
        *entry = at;
        self.templates.entry((key.node, key.depth)).or_insert_with(|| properties.clone());
        true
    }
}

#[derive(Default)]
pub(super) struct StampCache {
    slots: Vec<Option<(Key, Properties)>>,
    lookups: usize,
    images: PackedStamps<5>,
    placements: PackedStamps<4>,
    geometry: PackedStamps<8>,
}

impl StampCache {
    // Bound memory independently of the number of nodes, seeds and iterations in a package.
    const MAX_SLOTS: usize = 65_536;

    fn index(&self, key: Key) -> usize {
        let hash = (key.seed ^ key.node.rotate_left(7) ^ key.depth.rotate_left(19)).wrapping_mul(2_654_435_769);
        (hash >> 16) as usize & self.slots.len().saturating_sub(1)
    }

    pub fn get(&mut self, key: Key, storage: Storage) -> Option<Properties> {
        self.lookups = self.lookups.saturating_add(1);
        if self.slots.len() < Self::MAX_SLOTS && self.lookups > self.slots.len() * 2 {
            // A shallow or sparsely connected graph only needs a small table.
            self.slots.resize_with((self.slots.len() * 2).max(16), || None);
        }
        if let Some(Some((stored, value))) = self.slots.get(self.index(key))
            && *stored == key
        {
            return Some(value.clone());
        }
        match storage {
            Storage::Full | Storage::Geometry { terminal: false } => None,
            Storage::Geometry { terminal: true } => {
                let (mut properties, [bx, by, x, y, width, height, rotation, alpha]) = self.geometry.get(key)?;
                properties.branch_offset = [bx, by];
                properties.stamp.center = [x, y];
                properties.stamp.size = [width, height];
                properties.stamp.rotation = rotation;
                properties.stamp.color[3] = alpha;
                Some(properties)
            }
            Storage::FixedImage => {
                let (mut properties, [x, y, width, height, alpha]) = self.images.get(key)?;
                properties.branch_offset = [x, y];
                properties.stamp.size = [width, height];
                properties.stamp.color[3] = alpha;
                Some(properties)
            }
            Storage::Placement | Storage::BranchTint { .. } => {
                let (mut properties, [x, y, rotation, alpha]) = self.placements.get(key)?;
                if matches!(storage, Storage::BranchTint { .. }) {
                    properties.branch_offset = [x, y];
                } else {
                    properties.stamp.center = [x, y];
                }
                properties.stamp.rotation = rotation;
                properties.stamp.color[3] = alpha;
                Some(properties)
            }
        }
    }

    pub fn insert(&mut self, key: Key, properties: Properties, storage: Storage, streaming: bool) {
        if streaming || matches!(storage, Storage::BranchTint { .. } | Storage::Geometry { terminal: true }) {
            let stamp = &properties.stamp;
            // Both representations of analytic stamps share one byte limit. Keep sparse
            // parents in the small table so their seed pages cannot displace dense leaves.
            const ANALYTIC_BYTES: usize = 96 << 20;
            let stored = match storage {
                Storage::Geometry { terminal: true } if self.lookups > Self::MAX_SLOTS * 2 => {
                    let [bx, by] = properties.branch_offset;
                    let [x, y] = stamp.center;
                    let [width, height] = stamp.size;
                    let fields = [bx, by, x, y, width, height, stamp.rotation, stamp.color[3]];
                    self.geometry.insert(key, &properties, fields, ANALYTIC_BYTES.saturating_sub(self.placements.bytes))
                }
                Storage::FixedImage if self.lookups > Self::MAX_SLOTS * 2 => {
                    let [x, y] = properties.branch_offset;
                    let [width, height] = stamp.size;
                    self.images.insert(key, &properties, [x, y, width, height, stamp.color[3]], 64 << 20)
                }
                Storage::Placement | Storage::BranchTint { .. } => {
                    let [x, y] = if matches!(storage, Storage::BranchTint { .. }) { properties.branch_offset } else { stamp.center };
                    self.placements.insert(key, &properties, [x, y, stamp.rotation, stamp.color[3]], ANALYTIC_BYTES.saturating_sub(self.geometry.bytes))
                }
                _ => false,
            };
            if stored {
                return;
            }
        }
        let index = self.index(key);
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = Some((key, properties));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_programs_share_renumbered_connections_and_distinguish_rewired_operands() {
        use materialcraft_core::{FunctionGraph, PatternGraph, PatternNodeKind};

        let cases = [(1, [5, 9, 4], false, 1), (2, [32, 31, 33], false, 1), (3, [32, 31, 33], true, 3)].map(|(id, ids, reverse, expected)| {
            let mut offset = FunctionGraph::default();
            let a = offset.add_constant(Value::Float2([0.125, 0.25]));
            let b = offset.add_constant(Value::Float2([0.5, 0.75]));
            offset.add("sub", &[a, b], None);
            for (node, id) in offset.nodes.iter_mut().zip(ids) {
                node.id = id;
            }
            let [a, b, result] = ids;
            offset.nodes[2].inputs = if reverse { vec![Some(b), Some(a)] } else { vec![Some(a), Some(b)] };
            offset.output = Some(result);
            let mut node = PatternGraph::with_root(PatternNodeKind::Quadrant).nodes.remove(0);
            node.id = id;
            node.parameters.insert("branchoffset".into(), Value::Function(Box::new(offset)));
            (node, expected)
        });
        let mut groups = BranchGroups::default();
        for (node, expected) in &cases {
            assert!(matches!(groups.storage(node), Some(Storage::BranchTint { group, .. }) if group == *expected));
        }
    }

    #[test]
    fn streamed_geometry_keeps_each_translation_and_seed_dependent_field() {
        let mut cache = StampCache { lookups: StampCache::MAX_SLOTS * 2 + 1, ..Default::default() };
        let first = Key { node: 17, depth: 9, seed: 0x5611 };
        let second = Key { seed: first.seed | (7 << 16), ..first };
        let make = |v: f32| Properties {
            branch_offset: [v, -v],
            stamp: PatternStamp {
                center: [-2.0 * v, 2.0 * v],
                size: [v + 0.5, v + 0.75],
                rotation: -0.25 * v,
                literal_geometry: false,
                color: [1.0, 1.0, 1.0, v + 0.125],
                depth: 9,
                pattern: 3,
                variation: 0.25,
                blend: 0,
                filtering: 0,
                premultiplied: false,
            },
        };
        let storage = Storage::Geometry { terminal: true };
        for (key, v) in [(first, 0.125), (second, 0.375)] {
            cache.insert(key, make(v), storage, true);
        }
        for (key, v) in [(second, 0.375), (first, 0.125)] {
            let a = cache.get(key, storage).unwrap();
            let b = make(v);
            assert_eq!(a.branch_offset, b.branch_offset);
            assert_eq!((a.stamp.center, a.stamp.size, a.stamp.rotation, a.stamp.color), (b.stamp.center, b.stamp.size, b.stamp.rotation, b.stamp.color));
            assert_eq!((a.stamp.depth, a.stamp.pattern, a.stamp.variation), (9, 3, 0.25));
        }
        assert!(cache.get(Key { depth: 8, ..first }, storage).is_none());
        assert!(cache.get(Key { seed: first.seed | (1 << 24), ..first }, storage).is_none());
    }

    #[test]
    fn streamed_image_cache_distinguishes_seed_bits_and_drawing_depth() {
        let mut cache = StampCache { lookups: StampCache::MAX_SLOTS * 2 + 1, ..Default::default() };
        let first = Key { node: 17, depth: 9, seed: 0x5611 };
        let second = Key { seed: 0x815611, ..first };
        let mut properties = Properties {
            branch_offset: [0.125, -0.25],
            stamp: PatternStamp {
                center: [0.25, 0.125],
                size: [0.75, 0.25],
                rotation: -0.125,
                literal_geometry: false,
                color: [1.0, 1.0, 1.0, 0.375],
                depth: 9,
                pattern: 257,
                variation: 0.0,
                blend: 0,
                filtering: 1,
                premultiplied: false,
            },
        };
        cache.insert(first, properties.clone(), Storage::FixedImage, true);
        properties.branch_offset = [-0.375, 0.25];
        properties.stamp.size = [0.125, 0.875];
        properties.stamp.color[3] = 0.625;
        cache.insert(second, properties, Storage::FixedImage, true);
        let a = cache.get(first, Storage::FixedImage).unwrap();
        let b = cache.get(second, Storage::FixedImage).unwrap();
        assert_eq!((a.branch_offset, a.stamp.size, a.stamp.color[3]), ([0.125, -0.25], [0.75, 0.25], 0.375));
        assert_eq!((b.branch_offset, b.stamp.size, b.stamp.color[3]), ([-0.375, 0.25], [0.125, 0.875], 0.625));
        assert_eq!((b.stamp.center, b.stamp.rotation, b.stamp.pattern), ([0.25, 0.125], -0.125, 257));
        assert!(cache.get(Key { depth: 8, ..second }, Storage::FixedImage).is_none());
        assert!(cache.get(Key { node: 18, ..second }, Storage::FixedImage).is_none());
        assert!(cache.get(Key { seed: second.seed | (1 << 24), ..second }, Storage::FixedImage).is_none());
    }
}
