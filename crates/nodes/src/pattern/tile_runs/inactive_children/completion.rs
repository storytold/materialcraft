use super::{COPIES_PER_BATCH, ReceivingPair, SUBTREES, boundary, copies, pending_siblings};
use std::ops::Range;

struct IncomingPair {
    cut: u32,
    end: u32,
    deferred: Range<usize>,
    width: usize,
}

impl IncomingPair {
    fn after(ends: &[u32], used: &Range<u32>, last_group: usize) -> Option<Self> {
        let subtree = ends.partition_point(|&end| end < used.end);
        let (deferred, width) = pending_siblings(subtree)?;
        let end = boundary(ends, last_group * SUBTREES)?;
        let span = copies(ends, deferred.clone())?;
        if used.end > span.start || span.end > end {
            return None;
        }
        Some(Self { cut: used.end, end, deferred, width })
    }

    fn kept(&self, ends: &[u32]) -> Option<[Range<u32>; 2]> {
        let deferred = copies(ends, self.deferred.clone())?;
        Some([self.cut..deferred.start, deferred.end..self.end])
    }

    fn children(&self, ends: &[u32], offsets: Range<usize>) -> Option<Range<u32>> {
        let part = copies(ends, self.deferred.start + offsets.start..self.deferred.start + offsets.end)?;
        (part.end <= self.end).then_some(part)
    }
}

// When two depth-four siblings transfer together, each releases its first two
// children before the next root-quadrant pair. Their remaining children return
// to the streams that received the final pairs.
fn depth_four(
    ends: &[u32],
    first: &ReceivingPair,
    last: &ReceivingPair,
    incoming: &[IncomingPair; 2],
    middle_tail: Range<u32>,
    third: Range<u32>,
) -> Option<Vec<Vec<Range<u32>>>> {
    if first.pending.len() != 8 {
        return None;
    }
    let child = |offset: Range<usize>| copies(ends, first.pending.start + offset.start..first.pending.start + offset.end);
    let [lower, upper] = incoming;
    let [upper_head, upper_tail] = upper.kept(ends)?;
    let [lower_head, lower_tail] = lower.kept(ends)?;
    Some(vec![
        vec![copies(ends, upper.deferred.clone())?],
        vec![middle_tail, child(0..2)?, child(4..6)?],
        vec![copies(ends, 60 * SUBTREES..62 * SUBTREES)?, child(2..4)?, copies(ends, last.pending.clone())?],
        vec![copies(ends, 62 * SUBTREES..64 * SUBTREES)?, copies(ends, 48 * SUBTREES..50 * SUBTREES)?, child(7..8)?],
        vec![upper_head, upper_tail, copies(ends, lower.deferred.clone())?],
        vec![lower_head, lower_tail, child(6..7)?],
        vec![third, first.handoff.clone()?],
    ])
}

struct TransferredChildren {
    first: Vec<Range<u32>>,
    middle: Vec<Range<u32>>,
    last: Range<u32>,
    kept: Vec<Range<u32>>,
    before_middle_tail: Vec<Range<u32>>,
}

impl IncomingPair {
    fn transfer(&self, ends: &[u32], upper: bool) -> Option<TransferredChildren> {
        let mut first = Vec::new();
        let mut middle = Vec::new();
        let mut before_middle_tail = Vec::new();
        let mut kept = Vec::new();
        let last = match (self.width, self.deferred.len()) {
            (1, 2) => {
                first.push(self.children(ends, 0..1)?);
                kept.extend(self.kept(ends)?);
                self.children(ends, 1..2)?
            }
            (4, 4) => {
                // A whole depth-four sibling releases children of its own and
                // takes the first half of the next sibling before returning.
                let leading = self.children(ends, 0..2)?;
                if upper {
                    before_middle_tail.push(self.children(ends, 0..1)?);
                    first.push(self.children(ends, 3..4)?);
                    middle.push(self.children(ends, 1..2)?);
                    kept.push(self.children(ends, 2..3)?);
                } else {
                    first.push(self.children(ends, 2..4)?);
                    kept.push(leading.clone());
                }
                let last = self.children(ends, 4..6)?;
                kept.extend([self.cut..leading.start, last.end..self.end]);
                last
            }
            _ => return None,
        };
        Some(TransferredChildren { first, middle, last, kept, before_middle_tail })
    }
}

fn depth_five(
    ends: &[u32],
    first: &ReceivingPair,
    last: &ReceivingPair,
    incoming: &[IncomingPair; 2],
    middle_tail: Range<u32>,
    third: Range<u32>,
) -> Option<Vec<Vec<Range<u32>>>> {
    let siblings = |r: &Range<usize>| {
        (r.len() == 2).then_some(())?;
        Some([copies(ends, r.start..r.start + 1)?, copies(ends, r.start + 1..r.end)?])
    };
    let [first_left, first_right] = siblings(&first.pending)?;
    let [last_left, last_right] = siblings(&last.pending)?;
    let lower = incoming[0].transfer(ends, false)?;
    let upper = incoming[1].transfer(ends, true)?;
    let mut leading = lower.first;
    leading.extend(upper.before_middle_tail);
    let mut middle = vec![middle_tail, last_left];
    middle.extend(upper.first);
    let mut outer = vec![copies(ends, 60 * SUBTREES..62 * SUBTREES)?, first_left, last_right];
    outer.extend(upper.middle);
    let mut kept = upper.kept;
    kept.extend(lower.kept);
    kept.push(upper.last);
    Some(vec![
        leading,
        middle,
        outer,
        vec![copies(ends, 62 * SUBTREES..64 * SUBTREES)?, copies(ends, 48 * SUBTREES..50 * SUBTREES)?, first_right],
        kept,
        vec![third, first.handoff.clone()?, lower.last],
    ])
}

pub(super) fn split_at_copies(ranges: &[Range<u32>], mut count: u32) -> Option<[Vec<Range<u32>>; 2]> {
    let mut head = Vec::new();
    let mut tail = Vec::new();
    for range in ranges {
        let used = count.min(range.end.checked_sub(range.start)?);
        let split = range.start + used;
        if range.start < split {
            head.push(range.start..split);
        }
        if split < range.end {
            tail.push(split..range.end);
        }
        count -= used;
    }
    (count == 0).then_some([head, tail])
}

pub(super) fn ranges(
    ends: &[u32],
    receivers: &[ReceivingPair],
    halves: &[[Range<u32>; 2]; 2],
    third: Range<u32>,
    leading: &[Range<u32>],
    tails: &[Range<u32>],
) -> Option<Vec<Range<u32>>> {
    let [first, middle, last] = receivers else { return None };
    let incoming = [IncomingPair::after(ends, &halves[0][1], 54)?, IncomingPair::after(ends, &halves[1][1], 58)?];
    let groups = match first.split_width {
        4 => depth_four(ends, first, last, &incoming, middle.tail.clone(), third)?,
        1 => depth_five(ends, first, last, &incoming, middle.tail.clone(), third)?,
        _ => return None,
    };
    let mut submitted = leading.to_vec();
    let mut remaining = tails.to_vec();
    for (index, group) in groups.into_iter().enumerate() {
        let length = group.iter().try_fold(0_u32, |n, r| n.checked_add(r.end.checked_sub(r.start)?))?;
        if length < COPIES_PER_BATCH {
            remaining.extend(group.into_iter().filter(|r| !r.is_empty()));
            continue;
        }
        // The outer pair can fill on its own, after receiving the first
        // receiver's siblings, or after receiving the last receiver's siblings.
        // Those transfers happen after three, seven and eleven full batches.
        if index != 2 || length >= 2 * COPIES_PER_BATCH {
            return None;
        }
        let own = group.first()?;
        let incoming = group.get(1)?;
        let batches = if own.end - own.start >= COPIES_PER_BATCH {
            3
        } else if (own.end - own.start).checked_add(incoming.end - incoming.start)? >= COPIES_PER_BATCH {
            7
        } else {
            11
        };
        let [full, rest] = split_at_copies(&group, COPIES_PER_BATCH)?;
        let [mut before, after] = split_at_copies(&submitted, batches * COPIES_PER_BATCH)?;
        before.extend(full);
        before.extend(after);
        submitted = before;
        remaining.extend(rest);
    }
    submitted.extend(remaining);
    Some(submitted)
}
