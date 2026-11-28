// Exclusive endpoints in depth-seven parent visits and their submission ranks.
pub(in crate::pattern) const FOUR_CHILDREN: &[(usize, usize)] = &[
    (580, 6),
    (1024, 19),
    (1416, 15),
    (1536, 24),
    (2048, 20),
    (3140, 2),
    (4096, 9),
    (4676, 4),
    (5120, 13),
    (5376, 29),
    (5444, 18),
    (5632, 31),
    (5768, 10),
    (5888, 34),
    (6144, 14),
    (7236, 0),
    (8192, 11),
    (8772, 8),
    (9216, 25),
    (9352, 21),
    (9728, 27),
    (9984, 30),
    (10240, 26),
    (11332, 1),
    (12288, 22),
    (12800, 17),
    (13312, 3),
    (13448, 12),
    (13824, 33),
    (14336, 7),
    (14472, 23),
    (14848, 32),
    (15360, 5),
    (15872, 28),
    (16384, 16),
];

// When groups 58–59 and 0–1 fill their batch, group 2 is deferred. The next prefix
// starts in group 3, and groups 36–37 finish together. Interior cuts count tile copies.
pub(super) const FILLED_FIRST_PAIR: &[(usize, usize)] = &[
    (512, 6),
    (768, 21),
    (1860, 7),
    (2048, 24),
    (3140, 2),
    (4096, 10),
    (4676, 4),
    (5120, 19),
    (5376, 27),
    (5444, 16),
    (5632, 29),
    (5768, 11),
    (5888, 32),
    (6144, 20),
    (7236, 0),
    (8192, 12),
    (8772, 9),
    (9216, 22),
    (9728, 25),
    (9984, 28),
    (10240, 23),
    (11332, 1),
    (12288, 17),
    (12800, 15),
    (13312, 3),
    (13448, 13),
    (13824, 31),
    (14336, 8),
    (14472, 18),
    (14848, 30),
    (15360, 5),
    (15872, 26),
    (16384, 14),
];

#[cfg(test)]
mod tests {
    use super::super::{CopyPositions, DEPTH_THREE_PARENTS, DrawPosition, Region, TileCopies, UNIT_WINDOW};
    use super::FOUR_CHILDREN;

    #[test]
    fn a_tile_copy_filling_the_paired_subtrees_changes_the_following_four_child_runs() {
        for full in [false, true] {
            let mut copies = TileCopies {
                window: Region::new(UNIT_WINDOW).unwrap(),
                axes: [true; 2],
                parents_per_subtree: DEPTH_THREE_PARENTS,
                positions: CopyPositions::default(),
                subtree_ends: Vec::new(),
                full_pairs: 0,
            };
            for group in 0..64 {
                let extra = match group {
                    0 | 1 | 3 | 41..=43 | 59 => 85,
                    2 | 40 => 84,
                    58 => 84 + usize::from(full),
                    _ => 0,
                };
                // Move the marked leaves to the X seam, preserving the parent and first leaf for signed probes.
                let mut moved = (0..extra)
                    .map(|j| {
                        let leaf = j * 768 / extra;
                        leaf / 3 * 5 + leaf % 3 + 2
                    })
                    .peekable();
                for stamp in 0..1280 {
                    let count = 1 + u32::from(moved.next_if_eq(&stamp).is_some());
                    copies.positions.record(count, true).unwrap();
                }
                copies.subtree_ends.push(copies.positions.total);
            }
            let layout = copies.layout(FOUR_CHILDREN).unwrap();
            let bounds = copies.boundaries(&layout, 5).unwrap();
            let rank = |stamp| {
                let mark = DrawPosition { stamp, copy: 0 };
                layout.iter().zip(bounds.windows(2)).find_map(|(&(_, rank), pair)| (pair[0] <= mark && mark < pair[1]).then_some((rank, stamp))).unwrap()
            };
            // Each pair shared one pixel in two independent fields with its gains exchanged.
            for (first, second, short_order, full_order) in [
                (2560, 3840, true, false),
                (2900, 7675, true, false),
                (5120, 25000, false, true),
                (5125, 40960, false, true),
                (8800, 79360, false, true),
                (9500, 71680, true, false),
                (2561, 3841, true, false),
                (5121, 6401, true, true),
                (9001, 10281, false, false),
            ] {
                assert_eq!(rank(first) < rank(second), if full { full_order } else { short_order }, "full {full}, markers {first} and {second}");
            }
        }
    }
}
