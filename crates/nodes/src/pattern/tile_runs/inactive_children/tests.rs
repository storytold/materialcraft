use super::ranges;

// Each of the 16,384 parents draws before its four children. Move the first
// selected draws in one incoming pair to the tile edge, adding one copy each.
fn draw_ranks(edge_draws: usize) -> Vec<usize> {
    ranks_with_edges(50 * 1280 + 2, 23, edge_draws)
}

fn ranks_with_edges(first: usize, stride: usize, edge_draws: usize) -> Vec<usize> {
    ranks_for_copies(|draw| {
        let offset = draw.checked_sub(first);
        1 + u32::from(offset.is_some_and(|n| n.is_multiple_of(stride) && n / stride < edge_draws))
    })
}

fn ranks_for_copies(count: impl Fn(usize) -> u32) -> Vec<usize> {
    ranks_for_draws(81_920, count)
}

fn ranks_for_draws(draws: usize, count: impl Fn(usize) -> u32) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut copies = 0_u32;
    for draw in 0..draws {
        starts.push(copies);
        copies += count(draw);
        if (draw + 1).is_multiple_of(80) {
            ends.push(copies);
        }
    }
    let mut ranks = vec![usize::MAX; copies as usize];
    for (rank, copy) in ranges(&ends).unwrap().into_iter().flatten().enumerate() {
        assert_eq!(ranks[copy as usize], usize::MAX);
        ranks[copy as usize] = rank;
    }
    assert!(!ranks.contains(&usize::MAX));
    starts.into_iter().map(|copy| ranks[copy as usize]).collect()
}

#[test]
fn a_full_receiving_pair_reorders_the_following_two_batches() {
    // Each pair shared a pixel in two signed F32 renders. One extra edge copy
    // changes which half-quadrant submits its remaining draws first.
    for (edges, before) in [(19, true), (20, false)] {
        let rank = draw_ranks(edges);
        for (left, right) in [(66560, 59050), (36589, 57785), (39011, 58236), (67164, 57437), (40323, 57678)] {
            assert_eq!(rank[left] < rank[right], before, "{edges} edge copies, draws {left}, {right}");
        }
    }
}

#[test]
fn inactive_quadrants_submit_a_second_full_batch_from_the_same_half() {
    // Place 679 or 680 independent squares across the same tile edge. The last
    // extra copy fills the second batch before the next half starts drawing.
    for (extra, before) in [(679, false), (680, true)] {
        let mut copies = vec![1; 81_920];
        for i in 0..extra {
            copies[24 * 1280 + (1777 * i + 31) % 10240] = 2;
        }
        let ranks = ranks_for_copies(|draw| copies[draw]);
        for (left, right) in [(36173, 15446), (37269, 12312), (36456, 11047)] {
            assert_eq!(ranks[left] < ranks[right], before, "{extra} extra copies, draws {left}, {right}");
        }
    }
}

#[test]
fn inactive_quadrants_submit_a_full_outer_pair_before_the_receivers() {
    for (extra, before) in [(339, false), (340, true)] {
        let mut copies = vec![1; 81_920];
        for i in 0..extra {
            copies[62 * 1280 + (233 * i + 71) % 2560] = 2;
        }
        let ranks = ranks_for_copies(|draw| copies[draw]);
        for (left, right) in [(80736, 65485), (80128, 65100), (63952, 66509)] {
            assert_eq!(ranks[left] < ranks[right], before, "{extra} extra copies, draws {left}, {right}");
        }
    }
}

#[test]
fn receiving_tails_and_deferred_siblings_follow_the_full_batches() {
    let rank = draw_ranks(0);
    for (left, right, before) in [(29320, 59050, false), (2960, 72020, false), (50750, 18422, true), (3019, 29753, false)] {
        assert_eq!(rank[left] < rank[right], before, "draws {left}, {right}");
    }
}

#[test]
fn partial_streams_finish_in_the_order_their_remaining_siblings_arrive() {
    let ranks = ranks_with_edges(50 * 1280 + 2, 23, 0);
    for (left, right, before) in [(67293, 23561, true), (23911, 72552, false), (18729, 43941, false), (16934, 72444, false)] {
        assert_eq!(ranks[left] < ranks[right], before, "draws {left}, {right}");
    }
    let ranks = ranks_with_edges(50 * 1280 + 2, 23, 20);
    for (left, right, before) in [(24093, 77770, false), (79055, 73362, true), (18729, 44010, false), (80854, 23953, true)] {
        assert_eq!(ranks[left] < ranks[right], before, "draws {left}, {right}");
    }
}

#[test]
fn an_incoming_depth_four_sibling_can_pass_its_children_to_other_runs() {
    let ranks = ranks_with_edges(24 * 1280 + 1, 9, 40);
    for (left, right, before) in [(68062, 73183, false), (67972, 72940, false), (67359, 67445, false), (18729, 72736, false)] {
        assert_eq!(ranks[left] < ranks[right], before, "draws {left}, {right}");
    }
    let ranks = ranks_with_edges(40 * 1280 + 1, 9, 40);
    for (left, right, before) in [(67986, 72413, false), (77734, 23491, true), (79054, 67732, true), (67742, 72985, false)] {
        assert_eq!(ranks[left] < ranks[right], before, "draws {left}, {right}");
    }
}

#[test]
fn a_full_outer_pair_submits_when_its_last_required_copy_arrives() {
    // Move squares in the outer pair to the corner (three extra copies) or a
    // single edge (one extra copy). Twenty edge squares enter the first receiver.
    for (extra, pairs) in [
        (2580_usize, [(77040, 24132, true), (23563, 8764, true)]),
        (2739_usize, [(78431, 50530, true), (23572, 8764, true)]),
        (2899_usize, [(77666, 70663, true), (77953, 47021, true)]),
        (2900_usize, [(77683, 66417, true), (78114, 25244, true)]),
    ] {
        let mut copies = vec![1; 81_920];
        for i in 0..20 {
            copies[50 * 1280 + 2 + 23 * i] = 2;
        }
        for i in 0..extra / 3 + extra % 3 {
            let draw = 60 * 1280 + (211 * i + 17) % 2560;
            copies[draw] = if i < extra / 3 { 4 } else { 2 };
        }
        let ranks = ranks_for_copies(|draw| copies[draw]);
        for (left, right, before) in pairs {
            assert_eq!(ranks[left] < ranks[right], before, "{extra} copies, draws {left}, {right}");
        }
    }
}

#[test]
fn a_receiver_can_transfer_a_whole_sibling_and_parts_of_the_next() {
    let mut copies = vec![1; 81_920];
    for i in 0..360 {
        copies[50 * 1280 + (103 * i + 31) % 2560] = 2;
    }
    let ranks = ranks_for_copies(|draw| copies[draw]);
    for (left, right) in [(66559, 20480), (76799, 0), (71679, 40960)] {
        assert!(ranks[left] < ranks[right], "draws {left}, {right}");
    }
}

#[test]
fn the_third_half_fills_from_the_first_child_of_the_transferred_quarter() {
    // Change how many copies of that child fit, from all eighty down to one.
    for (extra, next) in [(600, 25200), (620, 25020), (679, 24961)] {
        let ranks = ranks_with_transfers(extra, 400);
        for (left, right) in [(30719, 2560), (51199, 25040), (25119, 9240), (69119, next)] {
            assert!(ranks[left] < ranks[right], "{extra} extra copies, draws {left}, {right}");
        }
    }
}

fn ranks_with_transfers(third_half: usize, last_partner: usize) -> Vec<usize> {
    let mut copies = vec![1; 81_920];
    for (group, width, amount) in [(24, 8, 680), (40, 8, 680), (8, 8, third_half), (50, 2, 400), (58, 2, 400), (54, 2, last_partner), (62, 2, 340)] {
        for i in 0..amount {
            copies[group * 1280 + (211 * i + 17) % (width * 1280)] = 2;
        }
    }
    ranks_for_copies(|draw| copies[draw])
}

#[test]
fn a_copy_from_the_second_child_changes_which_siblings_return_together() {
    // Reducing the third half by one copy makes the batch use both donor children.
    for (extra, before) in [(600, false), (599, true)] {
        let ranks = ranks_with_transfers(extra, 400);
        for (left, right, same_direction) in [(25152, 23943, true), (24708, 74015, false), (67104, 24733, true)] {
            assert_eq!(ranks[left] < ranks[right], before == same_direction, "{extra} extra copies, draws {left}, {right}");
        }
    }
    // The second child can also supply its last copy to the full batch.
    let ranks = ranks_with_transfers(520, 400);
    for (left, right) in [(51199, 25120), (25199, 9240), (25279, 24640)] {
        assert!(ranks[left] < ranks[right], "draws {left}, {right}");
    }
}

#[test]
fn the_last_receiver_can_return_one_or_two_smaller_subtrees() {
    for (third_half, partner, pairs) in
        [(620, 200, [(43759, 24560), (78049, 43726)]), (620, 260, [(78147, 43698), (43603, 24797)]), (551, 300, [(43755, 24508), (49378, 43635)])]
    {
        let ranks = ranks_with_transfers(third_half, partner);
        for (left, right) in pairs {
            assert!(ranks[left] < ranks[right], "third half {third_half}, partner {partner}, draws {left}, {right}");
        }
    }
}

fn ranks_with_outer_cut(field: usize, outer: usize) -> Vec<usize> {
    let mut copies = vec![1; 81_920];
    let regions = [
        (24, 8, 700 + 17 * field),
        (40, 8, 730 + 21 * field),
        (8, 8, 702 + 25 * field),
        (50, 2, 500 + 9 * field),
        (58, 2, 530 + 6 * field),
        (54, 2, 341 + 7 * field),
        (62, 2, outer),
    ];
    for (group, width, amount) in regions {
        for i in 0..amount {
            copies[group * 1280 + (1777 * i + 32 + field) % (width * 1280)] = 2;
        }
    }
    ranks_for_copies(|draw| copies[draw])
}

#[test]
fn a_deferred_outer_child_changes_which_pair_submits_first() {
    // The extra edge copy moves the cut across the boundary between two depth-five siblings.
    for (outer, before) in [(499, false), (500, true), (501, true)] {
        let ranks = ranks_with_outer_cut(3, outer);
        assert_eq!(ranks[69208] < ranks[64124], before, "{outer} extra copies");
    }
}

#[test]
fn the_cut_outer_child_resumes_between_two_transferred_siblings() {
    for (field, pairs) in
        [(3, [(63874, 63822), (44118, 63836), (63820, 63951), (63826, 44577)]), (7, [(63906, 63818), (44109, 63806), (63838, 63962), (63791, 44740)])]
    {
        let ranks = ranks_with_outer_cut(field, 501 + 8 * field);
        for (left, right) in pairs {
            assert!(ranks[left] < ranks[right], "field {field}, draws {left}, {right}");
        }
    }
}

#[test]
fn a_repeated_tree_fills_the_batch_left_by_the_outer_pair() {
    for extra in [0, 100, 259] {
        let mut copies = vec![1; 163_840];
        for i in 0..extra {
            copies[62 * 1280 + (173 * i + 37) % 2560] = 2;
        }
        let ranks = ranks_for_draws(copies.len(), |draw| copies[draw]);
        // The first tree supplies its outer pair and the last transferred child.
        // The next tree supplies exactly the copies still missing from the batch.
        let next_outer = 81_920 + 62 * 1280;
        let cut = next_outer + 260 - extra;
        assert!(ranks[23_599] < ranks[next_outer]);
        assert!(ranks[cut - 1] < ranks[67_250]);
        assert!(ranks[cut] > ranks[67_261]);
    }
}
