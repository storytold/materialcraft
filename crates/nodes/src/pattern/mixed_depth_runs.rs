use std::{borrow::Cow, sync::LazyLock};

use materialcraft_core::{PatternNode, PatternNodeKind};

use super::{
    pattern_stamp::{DrawPosition, DrawRun, PatternStamp},
    raster::MIN_STAMP_SIZE,
    tile_runs::{FOUR_CHILDREN, TileCopies, UNIT_WINDOW},
};
use crate::CookContext;

const DEPTH_SEVEN_PARENTS: usize = 4_usize.pow(7);

// Each pair is the exclusive end in depth-seven parent visits and its compositing position.
// The boundaries come from overlapping original squares. A prefix can end inside a grandchild.
const TWO_CHILDREN: &[(usize, usize)] = &[
    (1308, 8),
    (1536, 15),
    (1792, 14),
    (2048, 16),
    (3868, 2),
    (4096, 25),
    (5404, 4),
    (5632, 9),
    (5888, 18),
    (6144, 10),
    (7964, 0),
    (8192, 23),
    (9500, 6),
    (9728, 12),
    (9984, 11),
    (10240, 13),
    (12060, 1),
    (12288, 21),
    (12800, 20),
    (13312, 3),
    (13824, 24),
    (14336, 5),
    (14848, 22),
    (15360, 7),
    (15872, 17),
    (16384, 19),
];

// With four children and four grandchildren, the independent subtree boundaries
// are 64 parents apart. Runs ending between them fill one or more physical batches.
const FOUR_CHILDREN_AND_GRANDCHILDREN: &[(usize, usize)] = &[
    (64, 15),
    (192, 32),
    (1956, 16),
    (2048, 33),
    (3868, 6),
    (4096, 17),
    (4160, 9),
    (4288, 38),
    (6052, 10),
    (6144, 28),
    (7964, 2),
    (8192, 19),
    (8256, 12),
    (8384, 29),
    (10148, 13),
    (10240, 31),
    (12060, 4),
    (12288, 25),
    (12352, 23),
    (12416, 30),
    (12480, 43),
    (12684, 24),
    (12800, 39),
    (13060, 1),
    (13312, 8),
    (13376, 20),
    (13440, 34),
    (13504, 45),
    (13732, 21),
    (13824, 42),
    (14084, 3),
    (14336, 11),
    (14400, 26),
    (14464, 35),
    (14528, 40),
    (14756, 27),
    (14848, 41),
    (15108, 5),
    (15360, 14),
    (15620, 7),
    (15680, 36),
    (15712, 18),
    (15808, 44),
    (15872, 37),
    (16132, 0),
    (16384, 22),
];

// With two trees, each drawing parent and its descendants submit 21 draws before tiling.
// At the end of the first 30 runs a physical batch holds 92 + 128 parents.
// The next 40 finish that batch inside the run starting at parent 12,352.
const CONTINUED_PARENT: usize = 12_352 + 40;
fn continued_layout(prefix: &[(usize, usize)], common: usize, tail: &[usize]) -> Vec<(usize, usize)> {
    let mut layout: Vec<_> = prefix.iter().filter(|(_, rank)| *rank < common).copied().collect();
    layout.extend(tail.iter().enumerate().map(|(rank, &end)| (end, common + rank)));
    layout.sort_unstable();
    layout
}

static PAIRED_GRANDCHILDREN: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| {
    const COMMON_RUNS: usize = 30;
    // Exclusive parent counts in submission order. Counts above 16,384 belong
    // to the second tree. Signed overlaps measure where its batches take the
    // unfinished draws from the first tree, even when a parent misses the image.
    let tail = [
        CONTINUED_PARENT,
        12800,
        14528,
        32596,
        12416,
        29680,
        13824,
        12480,
        24452,
        10240,
        192,
        30508,
        14848,
        28612,
        2048,
        13440,
        14464,
        31532,
        15808,
        13504,
        20352,
        15680,
        15872,
        4288,
        31808,
        32140,
        29696,
        20736,
        22276,
        30720,
        24640,
        26572,
        31744,
        16448,
        18380,
        24576,
        29832,
        22528,
        24648,
        32768,
        28760,
        28672,
        30920,
        32256,
        20880,
        18432,
        30080,
        31056,
        28800,
        28928,
        29184,
        31132,
        29952,
        30208,
        29004,
        24768,
        28864,
        21068,
        20480,
        31936,
        30084,
        21120,
        26624,
        16576,
        31104,
        20992,
        21248,
        31168,
        30976,
        31232,
        21184,
        29056,
        30144,
    ];
    continued_layout(FOUR_CHILDREN_AND_GRANDCHILDREN, COMMON_RUNS, &tail)
});

// When five physical batches fit before parent 22,528, the rest of that
// subtree joins the two pending subtrees beginning at parent 24,640.
// Integral subtree ends are fixed. Interior markers select a subtree and
// the physical copy count locates the actual split inside it.
fn tiled_pair_continuation(five_batches: bool) -> Vec<(usize, usize)> {
    let cut = if five_batches { 22465 } else { 22273 };
    let mut tail = vec![cut, 30720, 24640, 26497, 31744, 16448, 18369, 20480, 31873, 24576, 29761];
    if !five_batches {
        // After four complete batches the remaining draws fill a fifth with
        // the first copies from the two pending subtrees.
        tail.extend([22528, 24641]);
    }
    tail.extend([32768, 28737, 28672, 30913, 32256, 20865, 28800, 28928, 29184, 31105, 29824, 29952, 30208, 28929]);
    if five_batches {
        tail.push(22528);
    }
    tail.extend([24768, 28864, 21120, 26624, 16576, 18432, 29888, 31104, 20992, 21248, 31168, 30976, 31232, 21184, 29056, 31936, 30080]);
    let mut layout = continued_layout(&PAIRED_GRANDCHILDREN, 58, &tail);
    for (end, rank) in &mut layout {
        // Two eight-batch cuts now fall in the preceding 64-parent subtree.
        match rank {
            38 => *end = 24385,
            43 => *end = 28545,
            _ => {}
        }
    }
    layout.sort_unstable();
    layout
}

static FIVE_BATCH_CONTINUATION: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| tiled_pair_continuation(true));
static FOUR_BATCH_CONTINUATION: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| tiled_pair_continuation(false));

enum ReceivingBatch {
    FirstHalf,
    SecondHalf,
    DeferredPair,
}

// Three pending subtrees start before the later quadrant when the cut
// before 18,432 falls before 18,368. If the pair ending at 31,936 cannot
// fill the receiving batch, it resumes in the run that returns to 21,120.
fn receiving_pair_continuation(receiver: ReceivingBatch) -> Vec<(usize, usize)> {
    let first_half = matches!(receiver, ReceivingBatch::FirstHalf);
    let deferred = matches!(receiver, ReceivingBatch::DeferredPair);
    let mut ordered = PAIRED_GRANDCHILDREN.to_vec();
    ordered.sort_unstable_by_key(|&(_, rank)| rank);
    let mut ends = Vec::with_capacity(ordered.len());
    for (end, _) in ordered {
        match end {
            // Physical batch ends can move into another 64-parent subtree.
            24452 if !first_half => ends.push(24449),
            28612 => ends.push(28545),
            22276 => ends.push(22465),
            18380 => ends.push(18305),
            29832 if first_half => ends.push(29823),
            30920 => ends.push(30849),
            31056 => ends.push(if first_half { 31041 } else { 30977 }),
            24576 if !deferred => ends.extend([20480, 31873, end]),
            30080 if first_half => ends.push(29888),
            29952 if first_half => ends.extend([29824, end]),
            24768 => ends.extend([22528, end]),
            // The pending pair can finish exactly at the end of its second subtree.
            21120 if deferred => ends.extend([20993, 20480, 31936, 30081, end]),
            30144 if !deferred => ends.extend([31936, if first_half { 30080 } else { end }]),
            // These parts moved above or joined the preceding full batch.
            22528 | 24648 | 21068 | 20480 | 31936 | 30084 => {}
            _ => ends.push(end),
        }
    }
    continued_layout(&[], 0, &ends)
}

static FIRST_HALF_RECEIVER: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| receiving_pair_continuation(ReceivingBatch::FirstHalf));
static SECOND_HALF_RECEIVER: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| receiving_pair_continuation(ReceivingBatch::SecondHalf));
static DEFERRED_PAIR: LazyLock<Vec<(usize, usize)>> = LazyLock::new(|| receiving_pair_continuation(ReceivingBatch::DeferredPair));

#[derive(Default)]
pub(super) struct MixedDepthRuns {
    layout: &'static [(usize, usize)],
    starts: Vec<DrawPosition>,
    next_boundary: usize,
    parents: usize,
    extra_parents: usize,
    trees: usize,
    children_per_parent: usize,
    draws_per_parent: usize,
    deepest: u32,
    draws: usize,
    shape: Option<i32>,
    fixed_image: bool,
    single_tree_window: bool,
    copies: Option<TileCopies>,
}

impl MixedDepthRuns {
    pub(super) fn new(context: &CookContext, deepest: u32, fixed_image: bool) -> Option<Self> {
        if context.b("cull_outside") {
            return None;
        }
        let axes = context.tiling_axes();
        // A common render and tiling window determines the repeated copies along its edges.
        let window = if axes == [true; 2] { context.v4("render_region") } else { UNIT_WINDOW };
        let copies = if axes != [false; 2] || deepest == 10 { Some(TileCopies::new(context, deepest, window)?) } else { None };
        Some(Self { copies, deepest, fixed_image, single_tree_window: deepest != 9 || window == UNIT_WINDOW, ..Self::default() })
    }

    pub(super) fn visit(&mut self, node: &PatternNode, depth: u32, retained: usize) -> bool {
        if node.kind != PatternNodeKind::Quadrant {
            return true;
        }
        // Count only repetitions that reach a quadrant. Two complete trees can
        // share a physical batch across the outer iteration.
        if depth == 0 {
            if self.trees != 0 {
                let continuation = match self.deepest {
                    9 => self.children_per_parent == 4 && self.copies.as_ref().is_none_or(TileCopies::tiles_both_axes),
                    10 => self.copies.as_ref().is_some_and(TileCopies::complete_inactive_tree),
                    _ => false,
                };
                if !continuation
                    || self.trees != 1
                    || self.extra_parents != 0
                    || self.parents != DEPTH_SEVEN_PARENTS
                    || self.draws != self.parents * self.draws_per_parent
                {
                    return false;
                }
            }
            self.trees += 1;
        }
        if depth != 7 {
            return match depth {
                5 if self.deepest == 10 => {
                    node.children.iter().all(Option::is_some) && self.copies.as_mut().is_some_and(|c| c.begin_inactive_subtree(self.trees))
                }
                0..=6 => node.children.iter().flatten().count() == 4,
                8 if self.deepest == 9 => node.children.iter().all(Option::is_some),
                // Connected child routes can terminate without visiting another quadrant.
                8 if self.deepest == 10 => true,
                9.. if self.deepest == 10 => false,
                depth if depth == self.deepest => node.children.iter().all(Option::is_none),
                _ => false,
            };
        }
        let children = node.children.iter().flatten().count();
        if self.children_per_parent == 0 {
            (self.layout, self.draws_per_parent) = match (self.deepest, children) {
                (8, 2) => (TWO_CHILDREN, 3),
                (8, 4) => (FOUR_CHILDREN, 5),
                (9, 4) => (PAIRED_GRANDCHILDREN.as_slice(), 21),
                (10, 4) => (&[][..], 5),
                _ => return false,
            };
            self.children_per_parent = children;
            self.starts.reserve_exact(self.layout.len() + 1);
        }
        if children != self.children_per_parent {
            return false;
        }
        if !self.layout.is_empty() && self.parents == self.next_boundary {
            let Some(&(end, _)) = self.layout.get(self.starts.len()) else { return false };
            self.starts.push(DrawPosition { stamp: retained, copy: 0 });
            self.next_boundary = end;
        }
        self.parents += 1;
        true
    }

    // A second visit to a drawing parent belongs to the same depth-five subtree.
    // An iteration at another level changes which subtrees can submit together.
    pub(super) fn repeat_parent(&mut self, depth: u32, repeats_left: u32) -> bool {
        if self.deepest != 10 || self.trees != 1 || depth != 7 || repeats_left != 1 || self.extra_parents == DEPTH_SEVEN_PARENTS {
            return false;
        }
        self.extra_parents += 1;
        true
    }

    pub(super) fn draw(&mut self, stamp: &PatternStamp, opacities: &[f32], retained: bool) -> bool {
        let Some(weight) = opacities.get(stamp.depth as usize) else { return false };
        let measured_shape = (2..=16).contains(&stamp.pattern) || (self.fixed_image && stamp.pattern >= 256);
        // Turning off unrelated draws changes the order even where those draws never overlap the readout.
        if !(7..=self.deepest).contains(&stamp.depth)
            || stamp.blend != 0
            || !measured_shape
            || *self.shape.get_or_insert(stamp.pattern) != stamp.pattern
            || !(stamp.color[3] * weight).is_normal()
            || stamp.size.iter().any(|axis| axis.abs() < MIN_STAMP_SIZE)
        {
            return false;
        }
        self.draws += 1;
        self.copies.as_mut().is_none_or(|copies| copies.record(stamp, retained, self.draws, self.draws_per_parent).is_some())
    }

    pub(super) fn finish(mut self, stamps: &[PatternStamp]) -> Option<Vec<DrawRun<'_>>> {
        if self.trees == 0 || self.parents != self.trees * DEPTH_SEVEN_PARENTS + self.extra_parents || self.draws != self.parents * self.draws_per_parent {
            return None;
        }
        if self.deepest == 9 && self.trees == 1 {
            // Every recorded draw has at least one copy. A wider window keeps the same
            // batch boundaries when none of those draws crosses a tiling edge.
            if !self.single_tree_window && self.copies.as_ref().is_none_or(|copies| copies.copy_count() != self.draws) {
                return None;
            }
            // Only a continuation splits this run. Rejoin the two retained
            // ranges before flushing the unfinished batches of a single tree.
            let split = self.layout.partition_point(|&(end, _)| end <= CONTINUED_PARENT);
            self.starts.get(split)?;
            self.starts.remove(split);
            self.layout = FOUR_CHILDREN_AND_GRANDCHILDREN;
        }
        if self.starts.len() != self.layout.len() {
            return None;
        }
        if self.deepest == 10 {
            return self.copies?.inactive_child_runs(stamps);
        }
        self.starts.push(DrawPosition { stamp: stamps.len(), copy: 0 });
        let mut layout = Cow::Borrowed(self.layout);
        if let Some(copies) = self.copies {
            if self.children_per_parent == 2 {
                return copies.two_child_runs(stamps);
            }
            if self.trees == 1 {
                layout = copies.layout(self.layout)?;
                self.starts = copies.boundaries(&layout, self.draws_per_parent)?;
            } else {
                let candidates = [
                    FIVE_BATCH_CONTINUATION.as_slice(),
                    FOUR_BATCH_CONTINUATION.as_slice(),
                    FIRST_HALF_RECEIVER.as_slice(),
                    SECOND_HALF_RECEIVER.as_slice(),
                    DEFERRED_PAIR.as_slice(),
                    self.layout,
                ];
                let (selected, bounds) =
                    candidates.into_iter().find_map(|candidate| copies.boundaries(candidate, self.draws_per_parent).map(|bounds| (candidate, bounds)))?;
                layout = Cow::Borrowed(selected);
                self.starts = bounds;
            }
        }
        let mut groups = vec![DrawRun::whole(&[]); layout.len()];
        for ((_, rank), bounds) in layout.iter().zip(self.starts.windows(2)) {
            *groups.get_mut(*rank)? = DrawRun::between(stamps, bounds[0], bounds[1])?;
        }
        Some(groups)
    }
}
