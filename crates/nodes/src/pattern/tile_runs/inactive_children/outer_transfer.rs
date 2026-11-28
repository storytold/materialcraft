use super::{SUBTREES, boundary, copies, finish_partial_streams, pending_siblings, receiving_pair, remainder};
use std::ops::Range;

pub(super) fn ranges(ends: &[u32], leading: &[Range<u32>], outer_tail: &Range<u32>) -> Option<Vec<Range<u32>>> {
    let [lower, upper, third, _, _] = leading else { return None };
    let lower = remainder(ends, 24, lower, 2)?;
    let upper = remainder(ends, 40, upper, 2)?;
    let third = remainder(ends, 8, third, 2)?;

    // When the cut leaves two children of the last outer quadrant, the next
    // child passes to another stream. The cut child stays with the last child.
    let cut_child = ends.partition_point(|&end| end < outer_tail.start);
    let (passed, width) = pending_siblings(cut_child)?;
    if width != 1 || passed != (cut_child + 1..cut_child + 2) || boundary(ends, passed.end + 1)? != outer_tail.end {
        return None;
    }
    let passed = copies(ends, passed)?;
    let kept = outer_tail.start..passed.start;
    let last_child = passed.end..outer_tail.end;

    // This handoff puts the pair sent to the third root quadrant first.
    // It passes a whole sibling, a half and a quarter to the unfinished streams.
    let first = receiving_pair(ends, 54, 32, true)?;
    let middle = receiving_pair(ends, 50, 16, false)?;
    let last = receiving_pair(ends, 58, 0, false)?;
    if first.split_width != SUBTREES || first.pending.len() != SUBTREES || [&middle, &last].iter().any(|pair| pair.split_width != 1 || pair.pending.len() != 1)
    {
        return None;
    }
    let child = |offset: Range<usize>| copies(ends, first.pending.start + offset.start..first.pending.start + offset.end);
    let mut submitted = leading.to_vec();
    submitted.extend(first.runs);
    submitted.extend(middle.runs);
    submitted.extend(last.runs);
    let streams = [
        vec![middle.tail, child(0..2)?, child(4..5)?],
        vec![first.tail, child(2..4)?],
        vec![copies(ends, last.pending)?, last.tail, child(16..18)?],
        vec![copies(ends, 60 * SUBTREES..62 * SUBTREES)?, passed, child(7..8)?],
        vec![kept, last_child, child(12..16)?, copies(ends, middle.pending)?, child(5..6)?],
        vec![upper, copies(ends, 56 * SUBTREES..58 * SUBTREES)?, child(8..12)?, child(6..7)?],
        vec![lower, copies(ends, 52 * SUBTREES..54 * SUBTREES)?, child(18..24)?],
        vec![third, child(24..28)?],
    ];
    finish_partial_streams(submitted, streams)
}
