use super::{COPIES_PER_BATCH, ReceivingPair, SUBTREES, copies, finish_partial_streams, remainder};
use std::ops::Range;

pub(super) fn ranges(ends: &[u32], receivers: &[ReceivingPair], leading: &[Range<u32>], outer_tail: &Range<u32>) -> Option<Vec<Range<u32>>> {
    let [first, middle, last] = receivers else { return None };
    let whole_sibling = |pair: &ReceivingPair| pair.split_width == SUBTREES && pair.pending.len() == SUBTREES;
    // The last receiver can also return one or two depth-five subtrees to the
    // same unfinished stream. Larger partial siblings can change its order.
    let last_returns_to_pair = whole_sibling(last) || last.split_width == 1 && last.pending.len() <= 2;
    if !whole_sibling(first) || !whole_sibling(middle) || !last_returns_to_pair {
        return None;
    }
    let [lower, upper, third, ..] = leading else { return None };
    let lower = remainder(ends, 24, lower, 2)?;
    let upper = remainder(ends, 40, upper, 2)?;
    let third = remainder(ends, 8, third, 1)?;

    // The first receiver transfers a whole sibling, then a half and a quarter
    // of the following sibling. Their descendants can finish different streams.
    let whole = first.pending.start;
    let half = first.pending.end;
    let quarter = half + SUBTREES / 2;
    let children = |start: usize, offsets: Range<usize>| copies(ends, start + offsets.start..start + offsets.end);
    let room = COPIES_PER_BATCH.checked_sub(third.end - third.start)?;
    let first_child = children(quarter, 0..1)?;
    // When the batch enters the second child, the middle receiver takes the
    // first child of the half. The upper receiver then takes its third child.
    let (donor_children, outer_children, returned_half, upper_child) =
        if room <= first_child.end - first_child.start { (1, 3..3, 2..3, children(quarter, 2..3)?) } else { (2, 3..4, 4..6, children(half, 2..3)?) };
    let donor = children(quarter, 0..donor_children)?;
    if room == 0 || room > donor.end - donor.start {
        return None;
    }
    let cut = donor.start + room;
    let mut runs = leading.to_vec();
    runs.extend([third, donor.start..cut, first.tail.clone(), copies(ends, middle.pending.clone())?]);
    let middle_start = donor_children - 1;
    if middle_start > 0 {
        runs.push(children(half, 0..middle_start)?);
    }
    runs.push(last.tail.clone());

    // After the third half fills, each unfinished stream receives its remaining
    // siblings. A stream that fills at this stage has a different submission time.
    let streams = [
        vec![children(quarter, donor_children..donor_children + 1)?],
        vec![middle.tail.clone(), children(whole, 0..8)?, children(half, middle_start..2)?],
        vec![copies(ends, 60 * SUBTREES..62 * SUBTREES)?, children(whole, 8..16)?, copies(ends, last.pending.clone())?],
        vec![outer_tail.clone(), children(half, outer_children)?, children(half, returned_half.end..8)?],
        vec![upper, copies(ends, 56 * SUBTREES..58 * SUBTREES)?, upper_child],
        vec![lower, copies(ends, 52 * SUBTREES..54 * SUBTREES)?],
        vec![cut..donor.end, children(quarter, 3..4)?, children(half, returned_half)?],
    ];
    finish_partial_streams(runs, streams)
}
