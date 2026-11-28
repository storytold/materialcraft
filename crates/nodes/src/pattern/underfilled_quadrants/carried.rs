use super::remaining::{Piece, Submission};
use super::{COLLECTING_GROUPS, COPIES_PER_BATCH, DrawPosition, LAST_QUADRANT_PAIR, LEADING_SUBTREES, RANGES_PER_CHILD, REPEATED_GROUPS, UnderfilledQuadrants};

impl UnderfilledQuadrants {
    pub(super) fn collecting_tail(&self, slot: usize) -> Option<[Piece<'_>; 3]> {
        let first = &self.collecting[0];
        let first_start = self.span_start(0, COLLECTING_GROUPS[0], 2)?;
        let branches = &first.pair.fourth_child_grandchildren;
        let group = &self.collecting[slot];
        let id = COLLECTING_GROUPS[slot];
        let start = self.span_start(0, id, 2)?;
        let tail = &group.spans[2];
        let pair = &group.pair;
        let fourth = &pair.fourth_child_branches;
        let handoff = self.collecting_handoff(slot);
        let mut skipped = handoff.end.checked_sub(handoff.start)?;
        let mut last = handoff.end;
        if slot == 0 {
            skipped = skipped.checked_add(fourth[3].checked_sub(fourth[0])?)?;
            last = fourth[3];
        }
        let size = u32::try_from(group.initial_copies()).ok()?.checked_add(tail.total.checked_sub(skipped)?)?;
        let remaining = size.checked_sub(2 * COPIES_PER_BATCH)?;
        if remaining > tail.total.checked_sub(last)? {
            return None;
        }

        let extra = match slot {
            0 => {
                let other = &self.collecting[2];
                (self.span_start(0, COLLECTING_GROUPS[2], 2)?, &other.spans[2], self.collecting_handoff(2))
            }
            2 => {
                let children = &first.pair.third_child_branches;
                (first_start, &first.spans[2], children[0]..children[2])
            }
            _ => (first_start, &first.spans[2], 0..0),
        };
        Some([(start, tail, tail.total - remaining..tail.total), extra, (first_start, &first.spans[2], branches[slot]..branches[slot + 1])])
    }

    pub(super) fn middle_tail(&self) -> Option<[Piece<'_>; 4]> {
        let first_start = self.span_start(0, LAST_QUADRANT_PAIR, 0)?;
        let first_group = &self.collecting[0];
        let second_group = &self.collecting[1];
        let first_branch = self.span_start(0, COLLECTING_GROUPS[0], 2)?;
        let second_branch = self.span_start(0, COLLECTING_GROUPS[1], 2)?;
        let children = &first_group.pair.third_child_branches;
        let last = &second_group.pair;
        let first = &self.last_quadrant_pair[0];
        Some([
            (first_start, first, 0..first.total),
            (first_branch, &first_group.spans[2], first_group.pair.third_child..children[0]),
            (first_branch, &first_group.spans[2], children[2]..first_group.pair.fourth_child),
            (second_branch, &second_group.spans[2], last.third_child..last.fourth_child),
        ])
    }

    pub(super) fn last_child_tail(&self) -> Option<[Piece<'_>; 4]> {
        let first = &self.collecting[0];
        let first_start = self.span_start(0, COLLECTING_GROUPS[0], 2)?;
        let branch = &first.pair;
        let grandchildren = &branch.fourth_child_grandchildren;
        let last = &self.collecting[3];
        let carried = u32::try_from(last.initial_copies()).ok()?.checked_add(last.spans[2].total)?.checked_sub(COPIES_PER_BATCH)?;
        let group = COLLECTING_GROUPS[3];
        Some([
            (self.span_start(0, group, 2)?, &last.spans[2], last.spans[2].total.checked_sub(carried)?..last.spans[2].total),
            (first_start, &first.spans[2], branch.fourth_child_branches[0]..grandchildren[0]),
            (first_start, &first.spans[2], grandchildren[3]..grandchildren[4]),
            (first_start, &first.spans[2], grandchildren[7]..branch.fourth_child_branches[2]),
        ])
    }

    pub(super) fn leading_pair_tail(&self, slot: usize) -> Option<(u32, [Submission; 3])> {
        let group = REPEATED_GROUPS[slot];
        let first_group = &self.collecting[0];
        let deferred = &first_group.pair.fourth_child_grandchildren[4..];
        let branch_start = self.span_start(0, COLLECTING_GROUPS[0], 2)?;
        let subtree = LEADING_SUBTREES[slot];
        let pair = self.pairs.get(subtree / 8)?;
        let repeated = &self.repeated[slot];
        let (middle_start, middle, range, branch) = if slot == 2 {
            let fourth = &first_group.pair.fourth_child_branches;
            (branch_start, &first_group.spans[2], fourth[2]..fourth[3], deferred[2]..deferred[3])
        } else {
            (self.span_start(0, group, 1)?, &repeated.middle, 0..repeated.middle.total, deferred[1 - slot]..deferred[2 - slot])
        };
        let tail = pair.copies.checked_sub(2 * COPIES_PER_BATCH)?;
        let carried = tail.checked_add(range.end.checked_sub(range.start)?)?.checked_add(branch.end.checked_sub(branch.start)?)?;
        let end = *self.starts.get((subtree / 16 + 1) * RANGES_PER_CHILD)?;
        Some((
            carried,
            [
                (pair.cut?, DrawPosition { stamp: end, copy: 0 }),
                Self::copy_range(middle_start, middle, range)?,
                Self::copy_range(branch_start, &first_group.spans[2], branch)?,
            ],
        ))
    }

    pub(super) fn append_single_tree_tail(&self, leading: &mut Vec<Submission>) -> Option<()> {
        // Without a following tree these unfinished batches drain in producer order.
        // A full transfer can submit earlier and change the order of the other producers.
        let mut pending = Vec::new();
        let (Some(first), Some(second), Some(third)) = (self.collecting_tail(0), self.collecting_tail(1), self.collecting_tail(2)) else { return Some(()) };
        let middle = self.middle_tail()?;
        let last_child = self.last_child_tail()?;
        for pieces in [&first[..], &second, &third, &middle, &last_child] {
            let count = Self::piece_count(pieces)?;
            if count >= COPIES_PER_BATCH {
                return Some(());
            }
            Self::append_copy_window(pieces, 0, count, &mut pending)?;
        }
        for slot in [1, 0, 2] {
            let (count, ranges) = self.leading_pair_tail(slot)?;
            if count >= COPIES_PER_BATCH {
                return Some(());
            }
            pending.extend(ranges);
        }
        leading.extend(pending);
        Some(())
    }
}
