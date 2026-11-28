use std::ops::Range;

use super::{Footprint, TileCopies, two_children};
use crate::CookContext;
use crate::pattern::{
    pattern_stamp::{DrawRun, PatternStamp},
    raster::MIN_STAMP_SIZE,
};
use materialcraft_core::{PatternNode, PatternNodeKind};

pub(in crate::pattern) struct SparseQuadrantRuns {
    copies: TileCopies,
    parents: Vec<Range<u32>>,
    groups: Vec<Range<u32>>,
    draws: usize,
    shape: Option<i32>,
    saw_zero: bool,
}

impl SparseQuadrantRuns {
    pub(in crate::pattern) fn new(context: &CookContext) -> Option<Self> {
        if context.b("cull_outside") || !matches!(context.tiling_axes(), [false, false] | [true, true]) {
            return None;
        }
        Some(Self { copies: TileCopies::new(context, 8, super::UNIT_WINDOW)?, parents: Vec::new(), groups: Vec::new(), draws: 0, shape: None, saw_zero: false })
    }

    pub(in crate::pattern) fn visit(&self, node: &PatternNode, depth: u32) -> bool {
        node.kind == PatternNodeKind::Quadrant
            && match depth {
                3 if self.parents.is_empty() => false,
                0..=7 => node.children.iter().flatten().count() == 4,
                8 => node.children.iter().all(Option::is_none),
                _ => false,
            }
    }

    pub(in crate::pattern) fn draw(&mut self, stamp: &PatternStamp, opacities: &[f32], retained: bool) -> Option<()> {
        let weight = *opacities.get(stamp.depth as usize)?;
        let empty = stamp.color[3] == 0.0;
        if !(2..=8).contains(&stamp.depth)
            || !(2..=16).contains(&stamp.pattern)
            || stamp.blend != 0
            || *self.shape.get_or_insert(stamp.pattern) != stamp.pattern
            || !weight.is_finite()
            || stamp.center.iter().chain(&stamp.size).chain([&stamp.rotation, &stamp.variation]).any(|value| !value.is_finite())
            || !(empty || (stamp.color[3].is_normal() && weight.is_normal() && (stamp.color[3] * weight).is_normal()))
            || stamp.size.iter().any(|axis| axis.abs() < MIN_STAMP_SIZE)
        {
            return None;
        }
        if empty {
            // Raster validation precedes its zero-luminosity skip.
            Footprint::new(stamp).ok()?;
            self.saw_zero = true;
        }
        self.draws += 1;
        let range = self.copies.count_stamp((!empty).then_some(stamp), retained)?;
        match stamp.depth {
            2 => self.parents.push(range),
            3 => self.groups.push(range),
            _ => self.groups.last_mut()?.end = range.end,
        }
        Some(())
    }

    pub(in crate::pattern) fn finish<'a>(self, stamps: &'a [PatternStamp]) -> Option<Vec<DrawRun<'a>>> {
        // A complete tree has 4^2 + ... + 4^8 authored draws. The measured sparse case
        // has at least one zero-luminosity draw, which consumes no batch capacity.
        if self.draws != 87_376 || self.parents.len() != 16 || self.groups.len() != 64 || !self.saw_zero {
            return None;
        }
        let mut ends = [0; 64];
        let mut pieces: Vec<(u32, Range<u32>)> = Vec::new();
        let mut submitted = 0_u32;
        for (group, children) in self.groups.into_iter().enumerate() {
            // In the final quadrant each depth-two parent precedes the second pair of
            // depth-three groups. Earlier parents precede the first pair.
            let slot = if group < 48 { 0 } else { 2 };
            let parent = (group % 4 == slot).then(|| self.parents[group / 4].clone());
            for range in parent.into_iter().chain([children]).filter(|r| !r.is_empty()) {
                let start = submitted;
                submitted = submitted.checked_add(range.end - range.start)?;
                if let Some((_, previous)) = pieces.last_mut()
                    && previous.end == range.start
                {
                    previous.end = range.end;
                } else {
                    pieces.push((start, range));
                }
            }
            ends[group] = submitted;
        }
        if submitted != self.copies.positions.total {
            return None;
        }

        let mut runs = Vec::new();
        for batch in two_children::ranges(&ends)? {
            for (offset, source) in &pieces {
                let first = batch.start.saturating_sub(*offset);
                let end = batch.end.saturating_sub(*offset).min(source.end - source.start);
                if first < end {
                    let start = self.copies.positions.at(source.start + first);
                    let stop = self.copies.positions.at(source.start + end);
                    runs.push(DrawRun::between(stamps, start, stop)?);
                }
            }
        }
        Some(runs)
    }
}
