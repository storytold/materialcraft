use std::ops::Range;

use materialcraft_core::{PatternNode, PatternNodeKind};

use super::{
    footprint::SampleGrid,
    pattern_stamp::{DrawRun, PatternStamp},
    raster::MIN_STAMP_SIZE,
    tile_runs::COPIES_PER_BATCH,
};

/// A path splits six subdivisions above the leaves. Its last connected child
/// continues the parent draw stream, including when that child draws nothing.
#[derive(Default)]
pub(super) struct QuadrantFork {
    depth: u32,
    children_left: Option<usize>,
    first_child: Option<usize>,
    last_child: Option<usize>,
    pair: bool,
    repeated: bool,
    earlier_children: Vec<Range<usize>>,
    pattern: Option<i32>,
    continuation_copies: u64,
}

impl QuadrantFork {
    pub(super) fn new(depth: u32) -> Self {
        Self { depth, ..Self::default() }
    }

    pub(super) fn visit(&mut self, node: &PatternNode, depth: u32, retained: usize) -> bool {
        if node.kind != PatternNodeKind::Quadrant {
            // An outer gate can emit several trees. Repetitions inside a child have additional draw streams.
            return depth == 0 || node.kind == PatternNodeKind::Dot;
        }
        if depth == 0 && self.children_left.is_some() {
            if !self.pair || self.finish_tree().is_none() {
                return false;
            }
            self.children_left = None;
            self.repeated = true;
        }
        if depth <= self.depth {
            if self.children_left.is_some() {
                return false;
            }
            let count = node.children.iter().flatten().count();
            if depth < self.depth {
                return count == 1;
            }
            self.pair = count == 2;
            self.children_left = Some(count);
            return count >= 2 && (!self.repeated || self.pair);
        }
        if depth == self.depth + 1 {
            let Some(left) = self.children_left.and_then(|count| count.checked_sub(1)) else { return false };
            self.children_left = Some(left);
            self.first_child.get_or_insert(retained);
            if left == 0 {
                self.last_child = Some(retained);
            }
        }
        true
    }

    pub(super) fn draw(&mut self, stamp: &PatternStamp, weights: &[f32], grid: Option<&SampleGrid>) -> bool {
        if stamp.blend != 0 || !(2..=16).contains(&stamp.pattern) || *self.pattern.get_or_insert(stamp.pattern) != stamp.pattern {
            return false;
        }
        if self.first_child.is_some() && self.last_child.is_none() {
            return true;
        }
        let Some(weight) = weights.get(stamp.depth as usize) else { return false };
        let gain = weight * stamp.color[3];
        if gain == 0.0 || stamp.size.iter().any(|side| side.abs() < MIN_STAMP_SIZE) {
            return true;
        }
        let Some(copies) = grid.and_then(|g| g.copies_per_axis(stamp)).and_then(|[x, y]| x.checked_mul(y)) else { return false };
        // Once the continuation submits a full batch, it can interleave with its sibling.
        // Count wrapped copies before discarding draws that miss the current sample grid.
        self.continuation_copies = self.continuation_copies.saturating_add(copies);
        gain.is_normal() && self.continuation_copies < u64::from(COPIES_PER_BATCH)
    }

    fn finish_tree(&mut self) -> Option<()> {
        let earlier = self.first_child.take()?..self.last_child.take()?;
        if !earlier.is_empty() {
            if let Some(last) = self.earlier_children.last_mut()
                && last.end == earlier.start
            {
                last.end = earlier.end;
            } else {
                self.earlier_children.push(earlier);
            }
        }
        Some(())
    }

    pub(super) fn runs(mut self, stamps: &[PatternStamp]) -> Option<Vec<DrawRun<'_>>> {
        self.finish_tree()?;
        if self.earlier_children.is_empty() {
            return None;
        }
        let mut runs = Vec::new();
        for range in &self.earlier_children {
            runs.push(DrawRun::whole(stamps.get(range.clone())?));
        }
        // Prefixes and last children share one stream across the emitted pairs.
        let mut cursor = 0;
        for range in self.earlier_children {
            if cursor < range.start {
                runs.push(DrawRun::whole(stamps.get(cursor..range.start)?));
            }
            cursor = range.end;
        }
        runs.push(DrawRun::whole(stamps.get(cursor..)?));
        Some(runs)
    }
}
