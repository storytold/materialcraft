use super::paired_batches::field_property;
use super::*;
use craft_texture::{Image, Kind};
use materialcraft_core::OutputFormat;

// Each depth-six parent draws before four children, each followed by four leaves.
// Address the three levels in separate columns and successive iterations in separate rows.
fn location(draw: u32) -> (u32, u32) {
    let iteration = draw / 86_016;
    let parent = draw % 86_016 / 21;
    let within = draw % 21;
    let (depth, visit, column) = match within {
        0 => (6, parent, 384),
        n if (n - 1) % 5 == 0 => (7, parent * 4 + (n - 1) / 5, 256),
        n => (8, parent * 16 + (n - 1) / 5 * 4 + (n - 2) % 5, 0),
    };
    let mut x = 0;
    let mut y = 0;
    for bit in 0..depth {
        x |= ((visit >> (2 * bit)) & 1) << bit;
        y |= ((visit >> (2 * bit + 1)) & 1) << bit;
    }
    (column + x, iteration * 256 + y)
}

fn check_terminal_levels(expanded: bool) {
    for (count, markers) in [
        (1, [(28193, 32245, false), (52923, 80165, false), (13176, 44146, true), (9090, 37313, false), (17308, 46423, true), (6792, 15213, false)]),
        (2, [(148776, 167500, false), (108044, 154830, false), (2356, 50934, true), (80910, 98000, false), (24344, 108669, true), (29011, 120962, false)]),
        (4, [(81374, 106292, false), (261307, 262710, false), (43010, 163056, true), (224189, 266224, false), (176006, 303409, true), (52157, 146973, false)]),
    ] {
        check_terminal_case(count, expanded, 0, 257831 + 19 * count as u32, &markers, None);
    }
}

fn check_terminal_case(count: i32, expanded: bool, extra_limit: u32, seed: u32, markers: &[(u32, u32, bool)], route: Option<(u32, PatternNodeKind)>) {
    // In a white pixel a tiny positive value survives only when the negative square draws first.
    let tiny = f32::EPSILON * 0.25;
    let root = if count == 1 { PatternNodeKind::Quadrant } else { PatternNodeKind::Iterate };
    let mut graph = PatternGraph::with_root(root);
    let mut parent = graph.root.unwrap();
    if count > 1 {
        parent = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].parameters.insert("numberadded".into(), set_then_return("tree_index", Value::Float(-1.0), Value::Int(count)));
        graph.nodes[(parent - 1) as usize].parameters.insert("branchoffset".into(), advance_tree_offset());
        graph.nodes[0].children[1] = Some(parent);
    }
    if let Some((0, kind)) = route {
        let child = graph.root;
        let relay = graph.add(kind).unwrap();
        graph.nodes[(relay - 1) as usize].children[0] = child;
        graph.root = Some(relay);
    }
    for depth in 1..=8 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(child));
        if let Some((level, kind)) = route
            && depth == level
        {
            let relay = graph.add(kind).unwrap();
            graph.nodes[(relay - 1) as usize].children[0] = Some(child);
            graph.nodes[(parent - 1) as usize].children[0] = Some(relay);
        }
        if depth >= 6 {
            let column = match depth {
                6 => 384,
                7 => 256,
                _ => 0,
            };
            let offset = [column as f32 / 1024.0, 0.0];
            let mut properties = additive_square(0.25, [0.0; 2], [0.0; 2]);
            properties.insert("frameoffset".into(), field_property(depth, offset, true, count > 1));
            properties.insert("opacity".into(), field_property(depth, offset, false, count > 1));
            graph.nodes[(child - 1) as usize].parameters = properties;
        }
        parent = child;
    }
    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
            ("background".into(), Value::Color([1.0; 4])),
            ("tiling_region".into(), Value::Float4(if expanded { [-0.5, 1.5, -0.5, 1.5] } else { [0.0, 1.0, 0.0, 1.0] })),
        ]),
    );
    let axes = if expanded { count - 1 } else { i32::from(extra_limit > 0) };
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(axes))]);
    let mut field = BTreeMap::new();
    if extra_limit > 0 {
        for group in 0..count as u32 * 64 {
            let extras = 1 + (7 * group + 3) % extra_limit;
            for draw in 1344 * group..1344 * group + extras {
                field.insert(location(draw), [0.0, 0.90625, 0.25, 1.0]);
            }
        }
    }
    for (j, &(negative, positive, _)) in markers.iter().enumerate() {
        let x = (8.5 + j as f32) / 128.0;
        field.insert(location(negative), [x, 12.5 / 128.0, -1.0, 1.0]);
        field.insert(location(positive), [x, 12.5 / 128.0, tiny, 1.0]);
    }
    let input = Image::generate(1024, 1024, Kind::Color, |x, y| field.get(&(x, y)).copied().unwrap_or([0.875, 0.90625, 0.25, 1.0]));
    let inputs = [None, Some(input)];
    let context = CookContext {
        w: 128,
        h: 128,
        seed,
        format: OutputFormat::F32,
        tile: true,
        inputs: &inputs,
        input_formats: &[OutputFormat::F32; 2],
        params: &params,
        resource: &|_| None,
        vars: &variables,
    };
    let image = (definition.cook)(&context).unwrap().remove(0);
    for (j, &(_, _, negative_first)) in markers.iter().enumerate() {
        assert_eq!(image.get_gray(8 + j as i64, 12, false), if negative_first { tiny } else { 0.0 }, "{count} trees, marker {j}");
    }
}

#[test]
fn terminal_quadrants_preserve_draw_order_through_an_outer_route() {
    let markers = [(148776, 167500, false), (2356, 50934, true), (29011, 120962, false)];
    for route in [PatternNodeKind::Dot, PatternNodeKind::Switch] {
        check_terminal_case(2, false, 0, 255151, &markers, Some((0, route)));
    }
}

#[test]
fn an_interior_route_preserves_terminal_draw_order() {
    let markers = [(148776, 167500, false), (2356, 50934, true), (29011, 120962, false)];
    for (level, kind) in [(1, PatternNodeKind::Dot), (3, PatternNodeKind::Switch), (8, PatternNodeKind::Dot)] {
        check_terminal_case(2, false, 0, 255151, &markers, Some((level, kind)));
    }
}

#[test]
fn three_terminal_levels_interleave_repeated_quadrant_trees() {
    check_terminal_levels(false);
}

#[test]
fn single_copy_terminal_trees_use_the_same_order_in_wide_tiling_regions() {
    check_terminal_levels(true);
}

#[test]
fn partly_wrapped_terminal_quadrants_fill_each_physical_batch() {
    for (count, limit, seed, markers) in [
        (1, 5, 275418, [(29710, 61959, false), (69391, 59206, true), (70042, 81747, false), (85417, 29721, true), (48532, 62040, false), (83643, 70043, true)]),
        (
            2,
            14,
            275459,
            [(109064, 165953, false), (164259, 152021, true), (37714, 69543, false), (109902, 106389, true), (148173, 169808, false), (112642, 94347, true)],
        ),
        (
            4,
            19,
            275541,
            [(179109, 248770, false), (207897, 148080, true), (6866, 144342, false), (327758, 324304, true), (109032, 131846, false), (180497, 179055, true)],
        ),
    ] {
        check_terminal_case(count, false, limit, seed, &markers, None);
    }
}

// A depth-seven parent followed by its four depth-eight children. Both coordinates address the procedural input in Morton order.
fn final_pair_location(draw: u32) -> (u32, u32) {
    let parent = draw / 5;
    let child = draw % 5;
    let axis = |shift: u32| (0..7).fold(0, |coordinate, bit| coordinate | (((parent >> (2 * bit + shift)) & 1) << bit));
    let (x, y) = (axis(0), axis(1));
    if child == 0 { (512 + x, y) } else { (2 * x + (child - 1) % 2, 2 * y + (child - 1) / 2) }
}

enum FinalPairTree {
    Plain,
    InactiveChildren,
    RepeatedChildren,
}

fn first_quadrant_repeat_offset() -> Value {
    let mut graph = FunctionGraph::default();
    let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let x = graph.add("swizzle1", &[pos], Some(Value::Int(0)));
    let y = graph.add("swizzle1", &[pos], Some(Value::Int(1)));
    let furthest = graph.add("max", &[x, y], None);
    let boundary = graph.add_constant(Value::Float(1.0 / 64.0));
    let first = graph.add("lr", &[furthest, boundary], None);
    let two = graph.add_constant(Value::Int(2));
    let one = graph.add_constant(Value::Int(1));
    let count = graph.add("ifelse", &[first, two, one], None);
    let set = graph.add("set", &[count], Some(Value::Text("child_repeat".into())));
    let offset = graph.add_constant(Value::Float2([0.0; 2]));
    graph.output = Some(graph.add("sequence", &[set, offset], None));
    Value::Function(Box::new(graph))
}

fn check_final_pair_markers(tree: FinalPairTree, wrapped: bool, origin: [f32; 2], markers: &[(u32, u32, bool)]) {
    let inactive_children = !matches!(tree, FinalPairTree::Plain);
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let mut parent = graph.root.unwrap();
    for depth in 1..=8 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(child));
        if depth == 7 && matches!(tree, FinalPairTree::RepeatedChildren) {
            // Only the first depth-six quadrant submits each of its children twice.
            let relay = graph.add(PatternNodeKind::Iterate).unwrap();
            let mut count = FunctionGraph::default();
            count.output = Some(count.add("get_integer1", &[], Some(Value::Text("child_repeat".into()))));
            graph.nodes[(relay - 1) as usize].parameters.insert("numberadded".into(), Value::Function(Box::new(count)));
            graph.nodes[(relay - 1) as usize].children[1] = Some(child);
            graph.nodes[(parent - 1) as usize].parameters.insert("branchoffset".into(), first_quadrant_repeat_offset());
            graph.nodes[(parent - 1) as usize].children.fill(Some(relay));
        }
        if depth >= 7 {
            let offset = [if depth == 7 { 0.5 } else { 0.0 }, 0.0];
            let mut parameters = additive_square(0.25, [0.0; 2], [0.0; 2]);
            parameters.insert("patternsize".into(), Value::Float2([0.875 * (1 << depth) as f32 / 128.0; 2]));
            parameters.insert("frameoffset".into(), field_property(depth, offset, true, false));
            parameters.insert("opacity".into(), field_property(depth, offset, false, false));
            graph.nodes[(child - 1) as usize].parameters = parameters;
        }
        parent = child;
    }
    if inactive_children {
        let gate = graph.add(PatternNodeKind::Switch).unwrap();
        let branch = graph.add(PatternNodeKind::Quadrant).unwrap();
        let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(gate));
        graph.nodes[(gate - 1) as usize].children[1] = Some(branch);
        graph.nodes[(branch - 1) as usize].children[0] = Some(leaf);
        assert_eq!(graph.validated_depth().unwrap(), 10);
    }
    let mut field = BTreeMap::new();
    if wrapped {
        // The second child of every 64th parent crosses the left edge.
        for draw in (2..81_920).step_by(320) {
            field.insert(final_pair_location(draw), [0.0, 0.90625, 0.25, 1.0]);
        }
    }
    let tiny = f32::EPSILON * 0.25;
    for (j, &(negative, positive, _)) in markers.iter().enumerate() {
        let x = (8.5 + j as f32) / 128.0;
        field.insert(final_pair_location(negative), [x, 12.5 / 128.0, -1.0, 1.0]);
        field.insert(final_pair_location(positive), [x, 12.5 / 128.0, tiny, 1.0]);
    }
    let input = Image::generate(1024, 1024, Kind::Color, |x, y| {
        let [cx, cy, gain, alpha] = field.get(&(x, y)).copied().unwrap_or([0.875, 0.90625, 0.25, 1.0]);
        [cx + origin[0], cy + origin[1], gain, alpha]
    });
    let inputs = [None, Some(input)];
    let window = Value::Float4([origin[0], origin[0] + 1.0, origin[1], origin[1] + 1.0]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(if inactive_children { 3 } else { 1 }))]);
    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
            ("background".into(), Value::Color([1.0; 4])),
            ("render_region".into(), window.clone()),
            ("tiling_region".into(), window),
        ]),
    );
    let context = CookContext {
        w: 128,
        h: 128,
        seed: 560071,
        format: OutputFormat::F32,
        tile: true,
        inputs: &inputs,
        input_formats: &[OutputFormat::F32; 2],
        params: &params,
        resource: &|_| None,
        vars: &variables,
    };
    let image = (definition.cook)(&context).unwrap().remove(0);
    for (j, &(_, _, first)) in markers.iter().enumerate() {
        assert_eq!(image.get_gray(8 + j as i64, 12, false), if first { tiny } else { 0.0 }, "marker {j}");
    }
}

#[test]
fn final_pair_batches_precede_the_next_subtree_above_inactive_children() {
    check_final_pair_markers(
        FinalPairTree::InactiveChildren,
        false,
        [0.0; 2],
        &[
            (36179, 51200, true),
            (36180, 51209, false),
            (30717, 30722, false),
            (56659, 10240, true),
            (56660, 10249, false),
            (51197, 51203, false),
            (15699, 64000, true),
            (15700, 64009, false),
            (10237, 10243, false),
            (23599, 24029, false),
            (74240, 79, true),
            (3119, 69149, false),
            (69120, 41039, true),
            (40959, 44109, false),
        ],
    );
    check_final_pair_markers(
        FinalPairTree::InactiveChildren,
        true,
        [0.0; 2],
        &[(64000, 20559, true), (23599, 24029, false), (76101, 7902, true), (69817, 48750, true)],
    );
    check_final_pair_markers(FinalPairTree::InactiveChildren, false, [0.125, -0.25], &[(36180, 51209, false), (30717, 30722, false), (56659, 10240, true)]);
}

#[test]
fn inactive_quadrants_count_repeated_draws_in_their_own_subtree() {
    check_final_pair_markers(
        FinalPairTree::RepeatedChildren,
        false,
        [0.0; 2],
        &[(30719, 2880, true), (49479, 36180, true), (63999, 23520, true), (67359, 8980, true)],
    );
}

#[test]
fn final_pair_batches_keep_the_four_child_order_with_one_tiling_axis() {
    check_final_pair_markers(
        FinalPairTree::Plain,
        true,
        [0.0; 2],
        &[(43841, 15684, true), (2893, 41170, false), (7078, 57951, false), (27212, 9738, false), (28838, 38806, false), (46736, 72040, false)],
    );
    check_final_pair_markers(
        FinalPairTree::Plain,
        false,
        [0.0; 2],
        &[
            (44397, 27843, true),
            (27885, 71936, false),
            (4416, 39465, false),
            (36850, 15761, false),
            (30513, 57392, true),
            (47200, 36364, false),
            (73365, 18745, false),
            (1690, 24737, true),
            (28478, 43002, false),
            (70020, 5532, true),
            (29835, 4097, true),
            (7465, 38479, false),
        ],
    );
}
