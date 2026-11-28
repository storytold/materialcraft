use super::{COLLECTING_GROUPS, COPIES_PER_BATCH, CopyPositions, DrawPosition, Range, UnderfilledQuadrants};

mod next_tree;
mod transfers;

pub(super) type Submission = (DrawPosition, DrawPosition);
pub(super) type Piece<'a> = (usize, &'a CopyPositions, Range<u32>);

#[derive(Default)]
pub(super) struct SiblingOffsets {
    // Copy offsets at depths three, four and five, in quadrant traversal order.
    starts: [Vec<u32>; 3],
}

struct SiblingCut {
    bounds: [u32; 3],
    next_child: Option<u32>,
}

impl SiblingOffsets {
    pub(super) fn visit(&mut self, depth: u32, copies: u32) {
        if let Some(starts) = depth.checked_sub(3).and_then(|level| self.starts.get_mut(level as usize)) {
            starts.push(copies);
        }
    }

    fn unvisited_subtrees(&self, used: u32) -> usize {
        let subtrees = &self.starts[2];
        subtrees.len() - subtrees.partition_point(|&start| start < used)
    }

    fn children(&self, level: usize, first: usize, count: usize, sole_parent: bool, total: u32) -> Option<SiblingCut> {
        let starts = self.starts.get(level)?;
        let end = first + count;
        // The pending sibling can belong to the next pair. Its intersection with this span is empty.
        let offset = |index: usize| starts.get(index).copied().unwrap_or(total);
        Some(SiblingCut { bounds: [offset(first), offset(first + 1), offset(end)], next_child: sole_parent.then(|| offset(end + 1)) })
    }

    fn split(&self, used: u32, total: u32) -> Option<SiblingCut> {
        // A cut on the depth-two draw has no pending depth-three siblings yet.
        let mut branch = self.starts[0].partition_point(|&start| start < used).checked_sub(1)?;
        for (level, starts) in self.starts.iter().enumerate().skip(1) {
            let first = branch * 4;
            let children = starts.get(first..first + 4)?;
            let visited = children.partition_point(|&start| start < used);
            if visited == 0 {
                return self.children(level, first, 2, true, total);
            }
            branch = first + visited - 1;
        }
        for level in (0..self.starts.len()).rev() {
            let pending = 3 - branch % 4;
            if pending >= 2 {
                return self.children(level, branch + 1, pending.div_ceil(2), false, total);
            }
            if pending == 1 && level + 1 < self.starts.len() {
                // Keep the sole remaining parent's draw and defer its first two children.
                return self.children(level + 1, (branch + 1) * 4, 2, true, total);
            }
            // A depth-five subtree finishes as a unit. Continue up its pending sibling stack.
            branch /= 4;
        }
        None
    }
}

pub(super) struct RemainingCopies<'a> {
    start: usize,
    span: &'a CopyPositions,
    before: Range<u32>,
    deferred: [Range<u32>; 2],
    after: Range<u32>,
    next_child: Option<u32>,
}

impl<'a> RemainingCopies<'a> {
    pub(super) fn new(start: usize, span: &'a CopyPositions, branches: &SiblingOffsets, used: u32) -> Option<Self> {
        let cut = branches.split(used, span.total)?;
        let [first, middle, end] = cut.bounds;
        if !(used..=span.total).contains(&first) || end > span.total {
            return None;
        }
        Some(Self { start, span, before: used..first, deferred: [first..middle, middle..end], after: end..span.total, next_child: cut.next_child })
    }

    fn piece(&self, range: Range<u32>) -> Piece<'a> {
        (self.start, self.span, range)
    }

    fn kept(&self) -> [Piece<'a>; 2] {
        [self.piece(self.before.clone()), self.piece(self.after.clone())]
    }

    fn deferred(&self) -> [Piece<'a>; 2] {
        self.deferred.clone().map(|range| self.piece(range))
    }

    pub(super) fn submit_second(&mut self, leading: &mut Vec<Submission>) -> Option<bool> {
        let before = self.before.end.checked_sub(self.before.start)?;
        let after = self.after.end.checked_sub(self.after.start)?;
        if before.checked_add(after)? / COPIES_PER_BATCH != 1 {
            // A third full batch or a partially filled second submission can interleave with the next group.
            return Some(false);
        }
        UnderfilledQuadrants::append_copy_window(&self.kept(), 0, COPIES_PER_BATCH, leading)?;
        let from_before = before.min(COPIES_PER_BATCH);
        self.before.start += from_before;
        self.after.start += COPIES_PER_BATCH - from_before;
        Some(true)
    }
}

impl UnderfilledQuadrants {
    fn following_pair(&self, slot: usize, used: u32, leading: &mut Vec<Submission>) -> Option<RemainingCopies<'_>> {
        let group = *super::REPEATED_GROUPS.get(slot)?;
        let pair = self.repeated.get(slot)?;
        let start = self.span_start(1, group, 0)?;
        let tail = pair.next.total.checked_sub(used)?;
        let needed = COPIES_PER_BATCH.checked_sub(tail)?;
        let later = &pair.later;
        let later_start = self.span_start(1, group, 1)?;
        let remainder = RemainingCopies::new(later_start, later, &pair.branches, needed)?;
        let pieces = [(start, &pair.next, used..pair.next.total), (later_start, later, 0..later.total)];
        Self::append_copy_window(&pieces, 0, COPIES_PER_BATCH, leading)?;
        Some(remainder)
    }

    fn following_last_child(&self, leading: &mut Vec<Submission>) -> Option<RemainingCopies<'_>> {
        let [tail, own, middle, last] = self.last_child_tail()?;
        let [own_next, children, pair] = &self.next_collecting[3].spans;
        let group = COLLECTING_GROUPS[3];
        let pair_start = self.span_start(1, group, 2)?;
        let pieces = [
            tail,
            own,
            middle,
            last,
            (self.span_start(1, group, 0)?, own_next, 0..own_next.total),
            (self.span_start(1, group, 1)?, children, 0..children.total),
            (pair_start, pair, 0..pair.total),
        ];
        let before = Self::piece_count(&pieces[..6])?;
        let used = COPIES_PER_BATCH.checked_sub(before)?;
        let remainder = RemainingCopies::new(pair_start, pair, &self.next_collecting[3].branches, used)?;
        Self::append_copy_window(&pieces, 0, COPIES_PER_BATCH, leading)?;
        Some(remainder)
    }

    pub(super) fn piece_count(pieces: &[Piece<'_>]) -> Option<u32> {
        pieces.iter().try_fold(0u32, |sum, (_, _, range)| sum.checked_add(range.end.checked_sub(range.start)?))
    }

    fn append_receiver_and_last(first: &[Piece<'_>; 5], full_receiver: bool, mut last: Vec<Submission>, next: &mut Vec<Submission>) -> Option<()> {
        if full_receiver {
            // A receiver that needs the last group's handoff submits after that group's own batch.
            if Self::piece_count(&first[..4])? < COPIES_PER_BATCH {
                next.append(&mut last);
            }
            Self::append_copy_window(first, 0, COPIES_PER_BATCH, next)?;
        }
        next.extend(last);
        Some(())
    }

    pub(super) fn append_remaining_pairs(
        &self,
        collecting: &[RemainingCopies<'_>],
        pair_offsets: [u32; 3],
        middle_used: u32,
        leading: &mut Vec<Submission>,
    ) -> Option<()> {
        let [first_group, second_group, third_group] = collecting else { return Some(()) };
        let mut next = Vec::new();
        let Some(lower) = self.following_pair(0, pair_offsets[0], &mut next) else { return Some(()) };
        let mut last_batch = Vec::new();
        let Some(last) = self.following_last_child(&mut last_batch) else { return Some(()) };
        let mut upper_batch = Vec::new();
        let Some(upper) = self.following_pair(1, pair_offsets[1], &mut upper_batch) else { return Some(()) };
        let [second_left, second_right] = second_group.deferred();
        let [third_left, third_right] = third_group.deferred();
        let [first_left, first_right] = first_group.deferred();
        let [last_left, last_right] = last.deferred();
        let [lower_left, lower_right] = lower.deferred();
        let [upper_left, upper_right] = upper.deferred();
        let [first_head, first_tail] = first_group.kept();
        let [second_head, second_tail] = second_group.kept();
        let [third_head, third_tail] = third_group.kept();

        let first = [first_head, first_tail, second_left, second_right, last_left];
        let second = [second_head, second_tail, third_left, third_right];
        let third = [third_head, third_tail, lower_left, upper_left, upper_right];
        let received = Self::piece_count(&first)?;
        if received >= 2 * COPIES_PER_BATCH || Self::piece_count(&second)? >= COPIES_PER_BATCH || Self::piece_count(&third)? >= COPIES_PER_BATCH {
            return Some(());
        }

        let more_trees = self.visits[0] > 2;
        let full_receiver = received >= COPIES_PER_BATCH;
        if lower.deferred.iter().chain(&upper.deferred).all(Range::is_empty) {
            // With no pending sibling in either pair, both paired batches precede the last child.
            next.extend(upper_batch);
            Self::append_receiver_and_last(&first, full_receiver, last_batch, &mut next)?;
            if more_trees
                && full_receiver
                && let Some(batches) = self.carry_without_siblings(collecting, [&lower, &upper, &last])
            {
                next.extend(batches);
            }
        } else if full_receiver && Self::piece_count(&first[..2])? < COPIES_PER_BATCH && Self::piece_count(&first[..3])? >= COPIES_PER_BATCH {
            // The first child handed over by the second group completes the receiver.
            // How many siblings the lower pair gives up determines when that batch submits.
            match self.repeated[0].branches.unvisited_subtrees(lower.before.start) {
                2 if !lower.deferred[0].is_empty() && lower.deferred[1].is_empty() => {
                    next.extend(upper_batch);
                    // When both pairs transfer one of their final two subtrees,
                    // the last child finishes before the receiver.
                    let single_upper = self.repeated[1].branches.unvisited_subtrees(upper.before.start) == 2;
                    if single_upper {
                        next.append(&mut last_batch);
                    }
                    Self::append_receiver_and_last(&first, true, last_batch, &mut next)?;
                    if more_trees
                        && single_upper
                        && let Some(batches) = self.carry_single_transfers(collecting, [&lower, &upper, &last], pair_offsets[2])
                    {
                        next.extend(batches);
                    }
                }
                3 | 4 if lower.deferred.iter().all(|sibling| !sibling.is_empty()) => {
                    Self::append_copy_window(&first, 0, COPIES_PER_BATCH, &mut next)?;
                    next.extend(upper_batch);
                    next.extend(last_batch);
                    if let Some(tail) = self.unfinished_receivers(collecting, [&lower, &upper, &last], pair_offsets[2], middle_used) {
                        next.extend(tail);
                    }
                }
                _ => {}
            }
        } else if !more_trees && !full_receiver {
            next.extend(last_batch);
            next.extend(upper_batch);
        }
        if more_trees || full_receiver {
            // Another tree or an additional full receiver can interleave the unfinished groups.
            leading.extend(next);
            return Some(());
        }
        let pair = &self.last_quadrant_pair[1];
        let gap = &self.next_pair_gap;
        let start = self.span_start(1, super::LAST_QUADRANT_PAIR, 0)?;
        if middle_used > gap.start {
            return Some(());
        }
        let [last_head, last_tail] = last.kept();
        let [low_head, low_tail] = lower.kept();
        let middle = [(start, pair, middle_used..gap.start), (start, pair, gap.end..pair.total), first_left, first_right];
        let split = upper.next_child.unwrap_or(upper.after.start);
        if !(upper.after.start..=upper.after.end).contains(&split) {
            return Some(());
        }
        let upper_ranges = [upper.piece(upper.after.start..split), upper.piece(upper.before.clone()), upper.piece(split..upper.after.end)];
        let third_pair = &self.repeated[2].next;
        let final_ranges = [
            low_head,
            low_tail,
            last_right,
            (self.span_start(1, super::REPEATED_GROUPS[2], 0)?, third_pair, pair_offsets[2]..third_pair.total),
            (start, pair, gap.clone()),
            lower_right,
        ];
        for pieces in [&first[..], &second[..], &third[..], &middle[..], &[last_head, last_tail], &upper_ranges[..], &final_ranges[..]] {
            let count = Self::piece_count(pieces)?;
            Self::append_copy_window(pieces, 0, count, &mut next)?;
        }
        leading.extend(next);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_handoff_can_complete_a_receiving_batch() {
        let mut span = CopyPositions::default();
        for _ in 0..=COPIES_PER_BATCH {
            span.record(1, true).unwrap();
        }
        let position = |stamp| DrawPosition { stamp, copy: 0 };
        let last = (position(20_000), position(20_001));
        // The last group hands off 85 copies. Its own batch precedes a receiver that still needs them.
        for incoming in [COPIES_PER_BATCH - 1, COPIES_PER_BATCH, COPIES_PER_BATCH + 1] {
            let empty = (0, &span, 0..0);
            let first = [(0, &span, 0..incoming), empty.clone(), empty.clone(), empty, (10_000, &span, 0..85)];
            let mut runs = Vec::new();
            UnderfilledQuadrants::append_receiver_and_last(&first, true, vec![last], &mut runs).unwrap();
            assert_eq!(runs.first() == Some(&last), incoming < COPIES_PER_BATCH);
            assert_eq!(runs.last() == Some(&last), incoming >= COPIES_PER_BATCH);
        }
    }
}
