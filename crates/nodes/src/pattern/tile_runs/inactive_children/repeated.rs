use super::{COPIES_PER_BATCH, SUBTREES, completion, copies, fill_halves, pending_siblings, receive_pairs};
use std::ops::Range;

pub(super) fn submit(ends: &[u32], first_tree: &[u32], submitted: &mut Vec<Range<u32>>) -> Option<()> {
    if submitted.iter().any(|half| half.end - half.start != COPIES_PER_BATCH) {
        return None;
    }
    let group = |start, end| copies(ends, start * SUBTREES..end * SUBTREES);
    let outer = group(62, 64)?;
    let incoming = group(48, 50)?;
    if (outer.end - outer.start).checked_add(incoming.end - incoming.start)? >= COPIES_PER_BATCH {
        return None;
    }

    let receivers = receive_pairs(first_tree, submitted);
    let [first, middle, last] = receivers.as_slice() else { return None };
    if receivers.iter().any(|p| !matches!((p.split_width, p.pending.len()), (1, 2) | (4, 8))) {
        return None;
    }
    let halves = fill_halves(first_tree, first, submitted)?;
    let upper_cut = halves.pairs[1][1].end;
    let (upper_siblings, _) = pending_siblings(first_tree.partition_point(|&end| end < upper_cut))?;
    // The other outer pair can submit earlier if its incoming siblings fill it.
    let other_outer = group(60, 62)?;
    let possible = [copies(ends, first.pending.clone())?, copies(ends, last.pending.clone())?, copies(ends, upper_siblings)?];
    let outer_limit = possible.iter().try_fold(other_outer.end - other_outer.start, |sum, range| sum.checked_add(range.end - range.start))?;
    if outer_limit >= COPIES_PER_BATCH
        || first.tail.start < first.handoff.as_ref()?.end
        || middle.tail.start < copies(ends, middle.pending.clone())?.end
        || last.tail.start < copies(ends, last.pending.clone())?.end
    {
        return None;
    }

    // The last child of the transferred sibling stays with this outer pair.
    // Its unfinished batch continues in the same pair of the following tree.
    let last_child = copies(ends, first.pending.end.checked_sub(1)?..first.pending.end)?;
    let next_outer = group(64 + 62, 64 + 64)?;
    let [full, _] = completion::split_at_copies(&[outer, incoming, last_child, next_outer], COPIES_PER_BATCH)?;
    submitted.extend(full);
    Some(())
}
