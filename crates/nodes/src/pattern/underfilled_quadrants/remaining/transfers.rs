use super::{COPIES_PER_BATCH, Piece, Range, RemainingCopies, SiblingOffsets, Submission, UnderfilledQuadrants};
use crate::pattern::underfilled_quadrants::{LAST_QUADRANT_PAIR, REPEATED_GROUPS};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    First,
    Second,
    Third,
    Middle,
    Last,
    Upper,
    Lower,
    Final,
}

impl SiblingOffsets {
    fn pending_children(&self, used: u32, total: u32) -> Vec<Range<u32>> {
        let children = &self.starts[2];
        children[children.partition_point(|&start| start < used)..]
            .iter()
            .map(|&start| {
                // A shallower draw can separate adjacent depth-five subtrees. It stays with the parent.
                let end = self.starts.iter().filter_map(|level| level.get(level.partition_point(|&offset| offset <= start))).copied().min().unwrap_or(total);
                start..end
            })
            .collect()
    }
}

impl<'a> RemainingCopies<'a> {
    fn distribute_children(&self, children: &[Range<u32>], owner: Group, destinations: &[Group], groups: &mut [Vec<Piece<'a>>; 8]) {
        let mut kept = self.before.start;
        for (child, &destination) in children.iter().zip(destinations) {
            if destination != owner {
                groups[owner as usize].push(self.piece(kept..child.start));
                groups[destination as usize].push(self.piece(child.clone()));
                kept = child.end;
            }
        }
        groups[owner as usize].push(self.piece(kept..self.span.total));
    }
}

impl UnderfilledQuadrants {
    pub(super) fn unfinished_receivers(
        &self,
        collecting: &[RemainingCopies<'_>],
        pairs: [&RemainingCopies<'_>; 3],
        third_used: u32,
        middle_used: u32,
    ) -> Option<Vec<Submission>> {
        use Group::{Final, First, Last, Lower, Middle, Second, Third, Upper};
        let [first, second, third] = collecting else { return None };
        let [lower, upper, last] = pairs;
        let children = |offsets: &SiblingOffsets, pair: &RemainingCopies<'_>| offsets.pending_children(pair.before.start, pair.span.total);
        let low_children = children(&self.repeated[0].branches, lower);
        let upper_children = children(&self.repeated[1].branches, upper);
        let last_children = children(&self.next_collecting[3].branches, last);

        // The lower pair supplies the third group and the final group. The upper pair
        // first supplies the three receivers, then the middle group if another child is ready.
        let low_routes: &[_] = match low_children.len() {
            3 => &[Third, Final, Lower],
            4 => &[Third, Third, Final, Lower],
            _ => return None,
        };
        let upper_routes: &[_] = match upper_children.len() {
            5 if upper.deferred[0] == upper_children[1] && upper.deferred[1] == upper_children[2] => &[Upper, First, Second, Third, Upper],
            6 if upper.deferred[0] == upper_children[0] && upper.deferred[1].is_empty() => &[First, Upper, Second, Third, Middle, Upper],
            _ => return None,
        };
        if lower.deferred.as_slice() != &low_children[..2] || !(2..=5).contains(&last_children.len()) {
            return None;
        }
        let mut last_routes = vec![Last; last_children.len()];
        let left = last_children.iter().position(|child| child == &last.deferred[0])?;
        last_routes[left] = First;
        let penultimate = last_routes.len() - 2;
        if penultimate != left {
            let middle_available = !upper_routes.contains(&Middle);
            last_routes[penultimate] = if middle_available { Middle } else { Upper };
            if !last.deferred[1].is_empty() && last.deferred[1] != last_children[penultimate] {
                let right = last_children.iter().position(|child| child == &last.deferred[1])?;
                last_routes[right] = if middle_available { Middle } else { First };
                last_routes[penultimate] = Upper;
            }
        }

        // The first receiver filled partway through the second group's transferred child.
        // Split the pending siblings at that copy and keep the rest of this child in the receiver.
        let [(start, span, left), second_right] = second.deferred();
        let used = left.start.checked_add(COPIES_PER_BATCH.checked_sub(Self::piece_count(&first.kept())?)?)?;
        if !left.contains(&used) {
            return None;
        }
        let mut incoming = RemainingCopies::new(start, span, &self.next_collecting[1].branches, used)?;
        if incoming.after.start > left.end {
            return None;
        }
        incoming.after.end = left.end;

        let middle_pair = &self.last_quadrant_pair[1];
        let middle_start = self.span_start(1, LAST_QUADRANT_PAIR, 0)?;
        let gap = &self.next_pair_gap;
        if middle_used > gap.start {
            return None;
        }
        let final_pair = &self.repeated[2].next;
        let final_start = self.span_start(1, REPEATED_GROUPS[2], 0)?;
        let mut groups: [Vec<Piece<'_>>; 8] = Default::default();
        groups[First as usize].extend(incoming.kept());
        groups[First as usize].push(second_right);
        groups[Second as usize].extend(second.kept());
        groups[Second as usize].extend(third.deferred());
        groups[Third as usize].extend(third.kept());
        groups[Middle as usize].extend([(middle_start, middle_pair, middle_used..gap.start), (middle_start, middle_pair, gap.end..middle_pair.total)]);
        groups[Middle as usize].extend(first.deferred());
        groups[Final as usize].extend([(final_start, final_pair, third_used..final_pair.total), (middle_start, middle_pair, gap.clone())]);

        lower.distribute_children(&low_children, Lower, low_routes, &mut groups);
        upper.distribute_children(&upper_children, Upper, upper_routes, &mut groups);
        last.distribute_children(&last_children, Last, &last_routes, &mut groups);
        groups[Lower as usize].extend(incoming.deferred());
        let mut counts = [0; 8];
        for (group, count) in groups.iter().zip(&mut counts) {
            *count = Self::piece_count(group)?;
            if *count >= COPIES_PER_BATCH {
                // A group that fills again can transfer more work before the remaining groups finish.
                return None;
            }
        }
        if self.visits[0] > 2 {
            return self.carry_into_third_tree(&groups[Lower as usize], &groups[Upper as usize]);
        }
        let mut submissions = Vec::new();
        for (group, count) in groups.iter().zip(counts) {
            Self::append_copy_window(group, 0, count, &mut submissions)?;
        }
        Some(submissions)
    }
}
