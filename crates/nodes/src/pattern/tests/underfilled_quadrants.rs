use super::*;

fn overlapping_subtree_pixel(repetitions: i32, negative_gain: f32, wide_first_level: bool) -> f32 {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let root = graph.root.unwrap();
    if repetitions > 1 {
        let iterate = graph.add(PatternNodeKind::Iterate).unwrap();
        graph.root = Some(iterate);
        let node = &mut graph.nodes[(iterate - 1) as usize];
        node.children[1] = Some(root);
        node.parameters.insert("numberadded".into(), Value::Int(repetitions));
    }
    let mut tail = graph.add(PatternNodeKind::Quadrant).unwrap();
    for _ in 4..8 {
        let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(tail));
        tail = parent;
    }

    let mut parents = vec![(root, [0_usize; 2])];
    for depth in 1..=3 {
        let mut children = Vec::new();
        for (parent, [x, y]) in parents {
            for slot in 0..4 {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(parent - 1) as usize].children[slot] = Some(child);
                let cell = [2 * x + slot % 2, 2 * y + slot / 2];
                if depth == 1 && wide_first_level {
                    let node = &mut graph.nodes[(child - 1) as usize];
                    node.parameters = BTreeMap::from([
                        ("patterntype".into(), Value::Int(6)),
                        ("blendingmode".into(), Value::Int(0)),
                        ("patternsize".into(), Value::Float2([5.5 + slot as f32, 7.75 - 0.5 * slot as f32])),
                        ("patternrotation".into(), Value::Float([0.0, 0.125, 0.21875, 0.34375][slot])),
                        ("opacity".into(), Value::Float(2.0_f32.powi(-80))),
                    ]);
                }
                if depth == 3 {
                    let index = children.len();
                    let (target, gain) = match index {
                        50 => (0.25, negative_gain),
                        0 => (0.25, 2.0_f32.powi(-25)),
                        _ => (0.75, 0.125),
                    };
                    let shift = cell.map(|coordinate| target - (coordinate as f32 + 0.5) / 8.0);
                    let node = &mut graph.nodes[(child - 1) as usize];
                    node.parameters = additive_square(gain, [0.0; 2], shift);
                    node.children.fill(Some(tail));
                }
                children.push((child, cell));
            }
        }
        parents = children;
    }
    let image = cook_in_regions(graph, &BTreeMap::new(), 41381, [32; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    image.get_gray(8, 8, false)
}

#[test]
fn overlapping_subtrees_in_an_underfilled_eight_level_tree() {
    let small = 2.0_f32.powi(-25);
    assert_eq!(overlapping_subtree_pixel(1, -1.0, false).to_bits(), small.to_bits());
    // The three negative draws reach 0.25 before the three small additions.
    assert_eq!(overlapping_subtree_pixel(3, -0.25, false).to_bits(), (0.25 + 3.0 * small).to_bits());
}

#[test]
fn wide_first_level_stamps_keep_the_order_of_deeper_draws() {
    assert_eq!(overlapping_subtree_pixel(1, -1.0, true).to_bits(), 2.0_f32.powi(-25).to_bits());
}

enum Continuation {
    None,
    EmptySwitch,
    InactiveBranch(u32),
    ActiveTrees(i32),
    RoutedIteration(PatternNodeKind),
}

fn on_tree_pass(graph: &mut FunctionGraph, pass: i32) -> u32 {
    let index = graph.add("get_float1", &[], Some(Value::Text("tree_index".into())));
    let pass = graph.add_constant(Value::Float(pass as f32));
    graph.add("eq", &[index, pass], None)
}

fn corner_offset(pass: Option<i32>) -> FunctionGraph {
    let mut offset = FunctionGraph::default();
    let origin = offset.add_constant(Value::Float2([0.0; 2]));
    let center = offset.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let shift = offset.add("sub", &[origin, center], None);
    offset.output = Some(if let Some(pass) = pass {
        let active = on_tree_pass(&mut offset, pass);
        offset.add("ifelse", &[active, shift, origin], None)
    } else {
        shift
    });
    offset
}

fn quadrant_tail(graph: &mut PatternGraph, first_depth: usize, mut next: Option<u32>, mut leaf_offset: Option<FunctionGraph>) -> Option<u32> {
    for depth in (first_depth..=8).rev() {
        let id = graph.add(PatternNodeKind::Quadrant).unwrap();
        let node = &mut graph.nodes[(id - 1) as usize];
        node.parameters = additive_square(2.0_f32.powi(-76), [0.0; 2], [0.0; 2]);
        if depth == 8
            && let Some(offset) = leaf_offset.take()
        {
            node.parameters.insert("frameoffset".into(), Value::Function(Box::new(offset)));
        }
        node.children.fill(next);
        next = Some(id);
    }
    next
}

fn filled_tree_pixel(negative: (usize, usize), positive: (usize, usize), continuation: Continuation, wrapped_subtrees: &[usize]) -> f32 {
    marked_tree_pixel(negative, positive, continuation, wrapped_subtrees, None, None)
}

fn marked_tree_pixel(
    negative: (usize, usize),
    positive: (usize, usize),
    continuation: Continuation,
    wrapped_subtrees: &[usize],
    marker_iterations: Option<[i32; 2]>,
    wrap_iteration: Option<i32>,
) -> f32 {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let iterate = graph.root.unwrap();
    let root = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[(iterate - 1) as usize].children[1] = Some(root);
    if marker_iterations.is_some() || wrap_iteration.is_some() {
        let count = match continuation {
            Continuation::ActiveTrees(count) => count,
            _ => 2,
        };
        graph.nodes[(iterate - 1) as usize].parameters.insert("numberadded".into(), set_then_return("tree_index", Value::Float(0.0), Value::Int(count)));
        graph.nodes[(root - 1) as usize].parameters.insert("branchoffset".into(), advance_tree_offset());
    }
    let end = matches!(continuation, Continuation::EmptySwitch).then(|| graph.add(PatternNodeKind::Switch).unwrap());
    let marked_depth = negative.0.max(positive.0).max(3);
    let plain = quadrant_tail(&mut graph, marked_depth + 1, end, None);
    let wrapped = if wrapped_subtrees.is_empty() { plain } else { quadrant_tail(&mut graph, marked_depth + 1, end, Some(corner_offset(wrap_iteration))) };
    // Share the unmarked subtrees so a sixth-level marker fits in an ordinary pattern graph.
    let unmarked = quadrant_tail(&mut graph, marked_depth, end, None);
    let unmarked_wrapped =
        if wrapped_subtrees.is_empty() { unmarked } else { quadrant_tail(&mut graph, marked_depth, end, Some(corner_offset(wrap_iteration))) };
    let mut parents = vec![(root, [0_usize; 2])];
    for depth in 1..=marked_depth {
        let mut children = Vec::new();
        for (parent_index, (parent, [x, y])) in parents.into_iter().enumerate() {
            for slot in 0..4 {
                let ordinal = parent_index * 4 + slot;
                if depth == marked_depth && (depth, ordinal) != negative && (depth, ordinal) != positive {
                    graph.nodes[(parent - 1) as usize].children[slot] = if wrapped_subtrees.contains(&ordinal) { unmarked_wrapped } else { unmarked };
                    continue;
                }
                let id = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(parent - 1) as usize].children[slot] = Some(id);
                let cell = [2 * x + slot % 2, 2 * y + slot / 2];
                let node = &mut graph.nodes[(id - 1) as usize];
                if depth >= 2 {
                    let marker = (depth, ordinal);
                    let (gain, shift) = if marker == negative || marker == positive {
                        let shift = cell.map(|i| 0.265625 - (i as f32 + 0.5) / (1 << depth) as f32);
                        (if marker == negative { -1.0 } else { 2.0_f32.powi(-25) }, shift)
                    } else {
                        (2.0_f32.powi(-76), [0.0; 2])
                    };
                    node.parameters = additive_square(gain, [0.0; 2], shift);
                    if let Some(passes) = marker_iterations
                        && (marker == negative || marker == positive)
                    {
                        let mut select = FunctionGraph::default();
                        let active = on_tree_pass(&mut select, passes[usize::from(marker == positive)]);
                        let mark = select.add_constant(Value::Float(gain));
                        let ordinary = select.add_constant(Value::Float(2.0_f32.powi(-76)));
                        select.output = Some(select.add("ifelse", &[active, mark, ordinary], None));
                        node.parameters.insert("opacity".into(), Value::Function(Box::new(select)));
                    }
                }
                if depth == marked_depth {
                    node.children.fill(if wrapped_subtrees.contains(&ordinal) { wrapped } else { plain });
                }
                children.push((id, cell));
            }
        }
        parents = children;
    }
    let inactive_depth = match continuation {
        Continuation::InactiveBranch(depth) => Some(depth),
        Continuation::ActiveTrees(_) | Continuation::RoutedIteration(_) => Some(10),
        _ => None,
    };
    if let Some(depth) = inactive_depth {
        let mut child = graph.add(PatternNodeKind::Quadrant).unwrap();
        for _ in 0..depth {
            let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(parent - 1) as usize].children[0] = Some(child);
            child = parent;
        }
        let switch = graph.add(PatternNodeKind::Switch).unwrap();
        graph.nodes[(switch - 1) as usize].children = vec![Some(root), Some(child)];
        graph.nodes[(iterate - 1) as usize].children[1] = Some(switch);
    }
    if let Continuation::RoutedIteration(kind) = continuation {
        let relay = graph.add(kind).unwrap();
        graph.nodes[(relay - 1) as usize].children[0] = Some(iterate);
        graph.root = Some(relay);
    }
    let image = cook_in_regions(graph, &BTreeMap::new(), 41381, [32; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    image.get_gray(8, 8, false)
}

fn filled_quadrant_pixel(negative: usize, positive: usize, continuation: Continuation) -> f32 {
    filled_tree_pixel((2, negative), (2, positive), continuation, &[])
}

#[test]
fn successive_quadrant_batches_preserve_their_child_ranges() {
    for (negative, positive) in [((2, 14), (2, 8)), ((2, 1), (3, 2)), ((2, 13), (2, 8)), ((2, 9), (3, 34))] {
        assert_eq!(filled_tree_pixel(negative, positive, Continuation::EmptySwitch, &[]).to_bits(), 2.0_f32.powi(-25).to_bits());
    }
    assert_eq!(filled_quadrant_pixel(14, 8, Continuation::None).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn wrapped_leaves_can_fill_a_quadrant_batch_before_a_leading_pair() {
    assert_eq!(filled_tree_pixel((2, 14), (2, 2), Continuation::EmptySwitch, &[]).to_bits(), 0);
    assert_eq!(filled_tree_pixel((2, 14), (2, 2), Continuation::EmptySwitch, &[58]).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_repeated_quadrant_pair_submits_draws_from_both_trees() {
    for (negative, positive, passes) in [((3, 52), (3, 2), [1, 1]), ((2, 6), (3, 56), [2, 1]), ((2, 10), (3, 2), [2, 1])] {
        let sample = marked_tree_pixel(negative, positive, Continuation::EmptySwitch, &[], Some(passes), None);
        assert_eq!(sample.to_bits(), 2.0_f32.powi(-25).to_bits());
    }
}

#[test]
fn wrapped_copies_can_fill_a_third_batch_in_the_next_tree() {
    let pixel = |subtrees: &[usize]| marked_tree_pixel((3, 30), (2, 10), Continuation::EmptySwitch, subtrees, Some([2, 2]), Some(2));
    assert_eq!(pixel(&[25]).to_bits(), 2.0_f32.powi(-25).to_bits());
    assert_eq!(pixel(&[]).to_bits(), 0);
}

#[test]
fn a_second_tree_quadrant_precedes_the_last_first_tree_pair_at_depth_ten() {
    let pixel = marked_tree_pixel((2, 2), (3, 60), Continuation::InactiveBranch(10), &[], Some([2, 1]), None);
    assert_eq!(pixel.to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn outer_iteration_retains_its_batches_through_a_routing_node() {
    for kind in [PatternNodeKind::Switch, PatternNodeKind::Dot] {
        let pixel = marked_tree_pixel((2, 2), (3, 60), Continuation::RoutedIteration(kind), &[], Some([2, 1]), None);
        assert_eq!(pixel.to_bits(), 2.0_f32.powi(-25).to_bits());
    }
}

#[test]
fn a_carried_middle_pair_precedes_the_remaining_first_tree_quadrant() {
    let pixel = marked_tree_pixel((3, 60), (3, 2), Continuation::InactiveBranch(10), &[], Some([2, 1]), None);
    assert_eq!(pixel.to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_first_tree_batch_keeps_its_place_before_carried_quadrants() {
    let pixel = |negative, positive, passes| marked_tree_pixel(negative, positive, Continuation::InactiveBranch(10), &[34], Some(passes), Some(1));
    assert_eq!(pixel((3, 60), (2, 6), [1, 2]).to_bits(), 2.0_f32.powi(-25).to_bits());
    assert_eq!(pixel((2, 6), (3, 2), [2, 1]).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn the_next_collecting_quadrant_reaches_the_second_tree() {
    let pixel = marked_tree_pixel((3, 50), (3, 7), Continuation::InactiveBranch(10), &[], Some([2, 1]), None);
    assert_eq!(pixel.to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_full_copy_batch_defers_siblings_at_the_current_quadrant() {
    let pixel = |wrapped: &[usize]| marked_tree_pixel((5, 270), (5, 268), Continuation::InactiveBranch(10), wrapped, Some([2, 2]), Some(1));
    assert_eq!(pixel(&[]).to_bits(), 2.0_f32.powi(-25).to_bits());
    // Moving 64 leaves to a tile corner adds 192 copies before the same two markers.
    assert_eq!(pixel(&[32]).to_bits(), 0);
}

#[test]
fn later_full_batches_reach_subsequent_collecting_quadrants() {
    for (negative, positive, passes) in [((3, 17), (3, 7), [2, 1]), ((3, 35), (3, 33), [2, 2]), ((3, 58), (3, 1), [2, 2])] {
        let pixel = marked_tree_pixel(negative, positive, Continuation::InactiveBranch(10), &[], Some(passes), None);
        assert_eq!(pixel.to_bits(), 2.0_f32.powi(-25).to_bits());
    }
}

#[test]
fn a_collecting_batch_can_fill_before_the_next_quadrant_pair_starts() {
    let sample = |wrapped: &[usize]| marked_tree_pixel((5, 32), (5, 384), Continuation::InactiveBranch(10), wrapped, Some([1, 2]), Some(1));
    let small = 2.0_f32.powi(-25);
    assert!(sample(&[32, 33, 34, 35]) < small * 0.5);
    assert_eq!(sample(&[32, 33, 34, 35, 36]).to_bits(), small.to_bits());
}

#[test]
fn receiving_groups_submit_before_their_own_carried_pair() {
    let small = 2.0_f32.powi(-25);
    for (negative, positive, first, count) in [(880, 671, 864, 7), (943, 169, 928, 4)] {
        let wrapped: Vec<_> = (first..first + count).collect();
        let sample = |wrapped: &[usize]| marked_tree_pixel((5, negative), (5, positive), Continuation::ActiveTrees(2), wrapped, Some([2, 2]), Some(2));
        assert!(sample(&wrapped[..count - 1]) < small * 0.5);
        assert_eq!(sample(&wrapped).to_bits(), small.to_bits());
    }
}

#[test]
fn a_transferred_sibling_can_fill_the_receiving_group() {
    let sample = marked_tree_pixel((5, 939), (5, 169), Continuation::ActiveTrees(2), &[928], Some([2, 2]), Some(1));
    assert_eq!(sample.to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn carried_pairs_can_fill_before_the_following_tree_starts() {
    let small = 2.0_f32.powi(-25);
    for trees in [2, 3, 4] {
        let sample = marked_tree_pixel((3, 56), (3, 48), Continuation::ActiveTrees(trees), &[], Some([2, 2]), None);
        assert_eq!(sample.to_bits(), small.to_bits(), "{trees} trees");
    }
}

#[test]
fn a_receiving_batch_precedes_the_upper_pair_after_a_sibling_transfer() {
    let sample = |first, second, wrapped: &[usize]| marked_tree_pixel((5, first), (5, second), Continuation::ActiveTrees(2), wrapped, Some([2, 2]), Some(2));
    let small = 2.0_f32.powi(-25);
    assert_eq!(sample(896, 336, &[]).to_bits(), small.to_bits());
    // The 64 corner squares add 192 copies. The lower pair hands off two
    // siblings, and the receiving batch and upper pair both precede the last pair.
    assert_eq!(sample(336, 896, &[832]).to_bits(), small.to_bits());
    assert_eq!(sample(896, 768, &[832]).to_bits(), small.to_bits());
}

#[test]
fn subsequent_trees_keep_completed_quadrant_batches_ahead_of_unfinished_groups() {
    let small = 2.0_f32.powi(-25).to_bits();
    for trees in [3, 4] {
        let sample =
            |first, second| marked_tree_pixel((5, first), (5, second), Continuation::ActiveTrees(trees), &[832, 896, 897], Some([2, 2]), Some(2)).to_bits();
        assert_eq!(sample(896, 768), small, "{trees} trees");
        assert_eq!(sample(336, 80), small, "{trees} trees");
    }
}

#[test]
fn third_tree_pairs_fill_with_copies_carried_from_the_second_tree() {
    let small = 2.0_f32.powi(-25).to_bits();
    for (negative, positive) in [(384, 80), (640, 80)] {
        let sample = marked_tree_pixel((5, negative), (5, positive), Continuation::ActiveTrees(3), &[832, 896, 897], Some([3, 2]), Some(2));
        assert_eq!(sample.to_bits(), small);
    }
}

#[test]
fn third_tree_pairs_keep_single_sibling_transfers_ahead_of_waiting_draws() {
    let mut wrapped: Vec<_> = (3328..3331).chain(3584..3587).collect();
    let small = 2.0_f32.powi(-25).to_bits();
    for negative in [1536, 2560] {
        let sample = marked_tree_pixel((6, negative), (6, 320), Continuation::ActiveTrees(4), &wrapped, Some([3, 2]), Some(2));
        assert_eq!(sample.to_bits(), small);
    }
    let incoming = |wrapped: &[usize]| marked_tree_pixel((6, 2200), (6, 2560), Continuation::ActiveTrees(4), wrapped, Some([2, 3]), Some(2));
    assert_eq!(incoming(&wrapped).to_bits(), small);
    // Extra copies move the cut across one more depth-five child.
    wrapped.extend(2112..2115);
    assert!(incoming(&wrapped) < f32::from_bits(small) * 0.5);
}

#[test]
fn third_tree_receivers_take_children_when_neither_pair_has_a_pending_sibling() {
    let small = 2.0_f32.powi(-25).to_bits();
    for (negative, positive, passes) in [(551, 544, [2, 2]), (384, 80, [3, 2]), (640, 80, [3, 2])] {
        let sample = marked_tree_pixel((5, negative), (5, positive), Continuation::ActiveTrees(3), &[], Some(passes), None);
        assert_eq!(sample.to_bits(), small);
    }
}

#[test]
fn third_tree_receivers_continue_a_collecting_group_before_the_final_pair() {
    let wrapped: Vec<_> = (3328..3331).chain(3584..3587).collect();
    let small = 2.0_f32.powi(-25).to_bits();
    for (negative, positive) in [(3712, 1008), (512, 2494)] {
        let sample = marked_tree_pixel((6, negative), (6, positive), Continuation::ActiveTrees(4), &wrapped, Some([3, 2]), Some(2));
        assert_eq!(sample.to_bits(), small);
    }
}

#[test]
fn unfinished_quadrant_groups_receive_children_after_the_full_batches() {
    let sample = |first, second| marked_tree_pixel((5, first), (5, second), Continuation::ActiveTrees(2), &[832, 896, 897], Some([2, 2]), Some(2)).to_bits();
    let small = 2.0_f32.powi(-25).to_bits();
    // The upper pair gives a child to the first group. A sibling from that
    // group's own cut is drawn only after the middle quadrant has finished.
    assert_eq!(sample(924, 624), small);
    assert_eq!(sample(624, 924), 0);
    assert_eq!(sample(972, 541), small);
    assert_eq!(sample(541, 972), 0);
}

#[test]
fn earlier_sibling_transfers_keep_their_order_across_repeated_trees() {
    let small = 2.0_f32.powi(-25);
    for trees in [2, 3, 4] {
        let sample = marked_tree_pixel((5, 32), (5, 30), Continuation::ActiveTrees(trees), &[928], Some([1, 1]), Some(1));
        assert_eq!(sample.to_bits(), small.to_bits(), "{trees} trees");
    }
}

#[test]
fn a_full_quadrant_pair_transfers_its_pending_sibling() {
    let sample =
        |negative, positive, wrapped: &[usize]| marked_tree_pixel((5, negative), (5, positive), Continuation::ActiveTrees(1), wrapped, Some([1, 1]), Some(1));
    let small = 2.0_f32.powi(-25);
    assert!(sample(32, 30, &[]) < small * 0.5);
    assert_eq!(sample(32, 30, &[928]).to_bits(), small.to_bits());
    assert!(sample(30, 32, &[928]) < small * 0.5);
}

#[test]
fn a_single_tree_drains_the_middle_quadrant_before_the_carried_pair() {
    let sample = |negative, positive| marked_tree_pixel((3, negative), (3, positive), Continuation::ActiveTrees(1), &[], Some([1, 1]), None);
    assert_eq!(sample(60, 52).to_bits(), 2.0_f32.powi(-25).to_bits());
    assert_eq!(sample(52, 60).to_bits(), 0);
}

#[test]
fn pending_quadrants_finish_after_the_last_full_batches() {
    for ordinal in [336, 592] {
        let sample = marked_tree_pixel((5, ordinal), (5, 80), Continuation::InactiveBranch(10), &[624], Some([2, 2]), Some(1));
        assert_eq!(sample.to_bits(), 2.0_f32.powi(-25).to_bits());
    }
    let upper = marked_tree_pixel((5, 896), (5, 768), Continuation::InactiveBranch(10), &[624], Some([2, 2]), Some(1));
    assert_eq!(upper.to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn filled_quadrant_batches_submit_the_middle_pair_before_the_upper_pair() {
    assert_eq!(filled_quadrant_pixel(10, 2, Continuation::None).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_full_batch_collects_separate_ranges_of_the_last_quadrant() {
    assert_eq!(filled_quadrant_pixel(15, 4, Continuation::None).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_filled_batch_can_start_in_the_first_branch_of_the_last_quadrant() {
    assert_eq!(filled_quadrant_pixel(12, 0, Continuation::None).to_bits(), 2.0_f32.powi(-25).to_bits());
}

#[test]
fn a_closed_switch_below_the_final_quadrants_changes_the_next_full_batch() {
    let small = 2.0_f32.powi(-25).to_bits();
    assert_eq!(filled_quadrant_pixel(5, 0, Continuation::None).to_bits(), 0);
    assert_eq!(filled_quadrant_pixel(5, 0, Continuation::EmptySwitch).to_bits(), small);
    assert_eq!(filled_quadrant_pixel(0, 5, Continuation::EmptySwitch).to_bits(), 0);
}

#[test]
fn an_inactive_branch_contributes_its_connected_quadrant_depth() {
    assert_eq!(filled_quadrant_pixel(5, 0, Continuation::InactiveBranch(8)).to_bits(), 0);
    assert_eq!(filled_quadrant_pixel(5, 0, Continuation::InactiveBranch(9)).to_bits(), 2.0_f32.powi(-25).to_bits());
}
