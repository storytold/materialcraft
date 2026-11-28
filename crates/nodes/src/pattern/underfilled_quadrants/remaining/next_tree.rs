use super::{COPIES_PER_BATCH, CopyPositions, Piece, Range, RemainingCopies, SiblingOffsets, Submission, UnderfilledQuadrants};
use crate::pattern::underfilled_quadrants::{COLLECTING_GROUPS, LAST_QUADRANT_PAIR, REPEATED_GROUPS};

struct IncomingChildren<'a> {
    start: usize,
    span: &'a CopyPositions,
    pending: &'a [u32],
    parents: &'a [u32; 4],
    children: &'a [u32; 16],
}

impl<'a> IncomingChildren<'a> {
    fn piece(&self, range: Range<u32>) -> Piece<'a> {
        (self.start, self.span, range)
    }
}

impl SiblingOffsets {
    fn starts_in(starts: &[u32], range: Range<u32>) -> &[u32] {
        let remaining = &starts[starts.partition_point(|&n| n < range.start)..];
        &remaining[..remaining.partition_point(|&n| n < range.end)]
    }

    fn incoming_children<'a>(&'a self, first: &RemainingCopies<'_>, second: &RemainingCopies<'a>) -> Option<IncomingChildren<'a>> {
        let [(start, span, left), (_, _, right)] = second.deferred();
        // The receiving group filled in the first of two adjacent depth-three children.
        let siblings = &self.starts[0];
        let index = siblings.binary_search(&left.start).ok()?;
        if left.end != right.start || siblings.get(index + 1) != Some(&right.start) || siblings.get(index + 2).copied().unwrap_or(span.total) != right.end {
            return None;
        }
        let cut = left.start.checked_add(COPIES_PER_BATCH.checked_sub(UnderfilledQuadrants::piece_count(&first.kept())?)?)?;
        if cut > left.end {
            return None;
        }
        Some(IncomingChildren {
            start,
            span,
            pending: Self::starts_in(&self.starts[2], cut..left.end),
            parents: Self::starts_in(&self.starts[1], right.clone()).try_into().ok()?,
            children: Self::starts_in(&self.starts[2], right).try_into().ok()?,
        })
    }
}

impl UnderfilledQuadrants {
    fn append_third_tree_pair(&self, slot: usize, carried: &[Piece<'_>], submissions: &mut Vec<Submission>) -> Option<()> {
        let span = &self.repeated.get(slot)?.third_tree;
        let count = Self::piece_count(carried)?;
        if count >= COPIES_PER_BATCH || count.checked_add(span.total)? / COPIES_PER_BATCH != 2 {
            // Another full submission changes when the other pair can receive work.
            return None;
        }
        let mut pieces = carried.to_vec();
        pieces.push((self.span_start(2, REPEATED_GROUPS[slot], 0)?, span, 0..span.total));
        Self::append_copy_window(&pieces, 0, 2 * COPIES_PER_BATCH, submissions)
    }

    pub(super) fn carry_into_third_tree(&self, lower: &[Piece<'_>], upper: &[Piece<'_>]) -> Option<Vec<Submission>> {
        let mut submissions = Vec::new();
        self.append_third_tree_pair(0, lower, &mut submissions)?;
        self.append_third_tree_pair(1, upper, &mut submissions)?;
        Some(submissions)
    }

    pub(super) fn carry_without_siblings(
        &self,
        collecting: &[RemainingCopies<'_>],
        [lower, upper, last]: [&RemainingCopies<'_>; 3],
    ) -> Option<Vec<Submission>> {
        let [first, second, _] = collecting else { return None };
        if self.repeated[0].branches.unvisited_subtrees(lower.before.start) != 1
            || self.repeated[1].branches.unvisited_subtrees(upper.before.start) != 1
            || self.next_collecting[3].branches.unvisited_subtrees(last.before.start) != 2
            || last.deferred[0].is_empty()
            || !last.deferred[1].is_empty()
        {
            return None;
        }
        let incoming = self.next_collecting[1].branches.incoming_children(first, second)?;
        if incoming.pending.len() != 3 {
            return None;
        }
        let parents = incoming.parents;
        let children = incoming.children;
        let [lower_head, lower_tail] = lower.kept();
        let [upper_head, upper_tail] = upper.kept();
        // The lower pair visits the second parent and its final child before it receives
        // the first two children of the first parent. The upper pair keeps that parent's draw.
        let lower_pieces = [
            lower_head,
            lower_tail,
            incoming.piece(parents[1]..children[4]),
            incoming.piece(children[7]..parents[2]),
            incoming.piece(children[0]..children[2]),
        ];
        let upper_pieces = [upper_head, upper_tail, incoming.piece(parents[0]..children[0]), incoming.piece(children[2]..parents[1])];
        self.carry_into_third_tree(&lower_pieces, &upper_pieces)
    }

    pub(super) fn carry_single_transfers(
        &self,
        collecting: &[RemainingCopies<'_>],
        [lower, upper, last]: [&RemainingCopies<'_>; 3],
        final_used: u32,
    ) -> Option<Vec<Submission>> {
        let [first, second, third] = collecting else { return None };
        if self.next_collecting[3].branches.unvisited_subtrees(last.before.start) != 2
            || [upper, last].iter().any(|pair| pair.deferred[0].is_empty() || !pair.deferred[1].is_empty())
        {
            return None;
        }
        let incoming = self.next_collecting[1].branches.incoming_children(first, second)?;
        let [lower_head, lower_tail] = lower.kept();
        let [upper_head, upper_tail] = upper.kept();
        let lower_pieces = [lower_head, lower_tail, upper.piece(upper.deferred[0].clone())];
        let mut upper_pieces = vec![upper_head, upper_tail, last.piece(last.deferred[0].clone())];
        match incoming.pending.len() {
            3 => {
                // The upper pair also receives the third child of the second depth-four quadrant.
                upper_pieces.push(incoming.piece(incoming.children[6]..incoming.children[7]));
            }
            4 => {}
            _ => return None,
        }
        let mut submissions = self.carry_into_third_tree(&lower_pieces, &upper_pieces)?;
        if let Some(batches) = self.collecting_and_final_pair(third, lower, &incoming, final_used) {
            submissions.extend(batches);
        }
        Some(submissions)
    }

    fn collecting_and_final_pair(
        &self,
        third: &RemainingCopies<'_>,
        lower: &RemainingCopies<'_>,
        incoming: &IncomingChildren<'_>,
        final_used: u32,
    ) -> Option<Vec<Submission>> {
        let children = incoming.children;
        let [first_pending, next_pending, ..] = incoming.pending else { return None };
        let mut pieces = third.kept().to_vec();
        pieces.push(lower.piece(lower.deferred[0].clone()));
        pieces.push(incoming.piece(*first_pending..*next_pending));
        let parents = incoming.parents;
        let final_child = if incoming.pending.len() == 3 {
            pieces.push(incoming.piece(children[2]..children[3]));
            parents[2]..parents[3]
        } else {
            // The depth-three parent's draw stays with the receiver.
            parents[0]..parents[2]
        };
        let group = COLLECTING_GROUPS[2];
        let [own, child] = &self.third_collecting;
        pieces.push((self.span_start(2, group, 0)?, own, 0..own.total));
        let needed = COPIES_PER_BATCH.checked_sub(Self::piece_count(&pieces)?)?;
        if needed == 0 || needed >= child.total {
            // This order was measured with the batch ending inside its first incoming child.
            return None;
        }
        pieces.push((self.span_start(2, group, 1)?, child, 0..child.total));
        let mut submissions = Vec::new();
        Self::append_copy_window(&pieces, 0, COPIES_PER_BATCH, &mut submissions)?;
        let previous = &self.repeated[2].next;
        let middle = &self.last_quadrant_pair[1];
        let final_pieces = [
            (self.span_start(1, REPEATED_GROUPS[2], 0)?, previous, final_used..previous.total),
            (self.span_start(1, LAST_QUADRANT_PAIR, 0)?, middle, self.next_pair_gap.clone()),
            incoming.piece(final_child),
        ];
        self.append_third_tree_pair(2, &final_pieces, &mut submissions)?;
        Some(submissions)
    }
}
