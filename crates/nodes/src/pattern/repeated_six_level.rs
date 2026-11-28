use super::{
    footprint::SampleGrid,
    pattern_stamp::{CopyPositions, DrawRun, PatternStamp},
    raster::MIN_STAMP_SIZE,
    tile_runs::COPIES_PER_BATCH,
};

// Each first-level quadrant contains 256 depth-five parents with four terminal children each.
const DRAWS_PER_QUADRANT: usize = 256 * 5;

#[derive(Default)]
pub(super) struct RepeatedSixLevelRuns {
    shape: Option<i32>,
    draws: usize,
    positions: CopyPositions,
    quadrant_ends: Vec<u32>,
}

impl RepeatedSixLevelRuns {
    pub fn record(&mut self, stamp: &PatternStamp, weights: &[f32], grid: Option<&SampleGrid>, retained: bool) -> Option<()> {
        let depth = if self.draws.is_multiple_of(5) { 5 } else { 6 };
        let weight = *weights.get(depth)?;
        if stamp.depth != depth as u32
            || stamp.blend != 0
            || !matches!(stamp.pattern, 2 | 8)
            || *self.shape.get_or_insert(stamp.pattern) != stamp.pattern
            || !stamp.color[3].is_normal()
            || !weight.is_normal()
            || !(stamp.color[3] * weight).is_normal()
            || !stamp.center.iter().chain(&stamp.size).chain([&stamp.rotation, &stamp.variation]).all(|v| v.is_finite())
            || stamp.size.iter().any(|axis| axis.abs() < MIN_STAMP_SIZE)
        {
            return None;
        }
        let copies = grid?.copy_count(stamp)?;
        let end = self.positions.record(copies, retained)?.end;
        self.draws += 1;
        if self.draws.is_multiple_of(DRAWS_PER_QUADRANT) {
            self.quadrant_ends.push(end);
        }
        Some(())
    }

    pub fn finish<'a>(self, stamps: &'a [PatternStamp], trees: usize) -> Option<Vec<DrawRun<'a>>> {
        // Without nested iterations this count is possible only when every parent and all four leaves draw.
        if trees < 2 || trees.checked_mul(4 * DRAWS_PER_QUADRANT)? != self.draws {
            return None;
        }
        let mut ordered = Vec::new();
        let mut pending: [Vec<DrawRun<'a>>; 4] = std::array::from_fn(|_| Vec::new());
        let mut space = [COPIES_PER_BATCH; 4];
        let mut tree_start = 0;
        let run = |start, end| DrawRun::between(stamps, self.positions.at(start), self.positions.at(end));
        for &[a, b, c, d] in self.quadrant_ends.as_chunks::<4>().0 {
            let quadrants = [tree_start..a, a..b, b..c, c..d];
            tree_start = d;
            // Each quadrant can add at most 1,280 draws with four copies apiece, filling at most one batch.
            for slot in [3, 0, 1, 2] {
                let mut start = quadrants[slot].start;
                let end = quadrants[slot].end;
                if end - start >= space[slot] {
                    let cut = start + space[slot];
                    pending[slot].push(run(start, cut)?);
                    ordered.append(&mut pending[slot]);
                    start = cut;
                    space[slot] = COPIES_PER_BATCH;
                }
                if start < end {
                    pending[slot].push(run(start, end)?);
                    space[slot] -= end - start;
                }
            }
        }
        // Partly filled batches are drawn in reverse quadrant order, retaining tree order inside each one.
        ordered.extend(pending.into_iter().rev().flatten());
        Some(ordered)
    }
}
