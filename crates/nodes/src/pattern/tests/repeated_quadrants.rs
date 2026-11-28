use super::*;
use materialcraft_core::OutputFormat;

fn select_iteration(graph: &mut FunctionGraph, number: u32, iteration: i32, gain: f32, otherwise: u32) -> u32 {
    let expected = graph.add_constant(Value::Float(iteration as f32));
    let selected = graph.add("eq", &[number, expected], None);
    let value = graph.add_constant(Value::Float(gain));
    graph.add("ifelse", &[selected, value, otherwise], None)
}

fn field(slot: usize, markers: [i32; 3], negative: f32, positive: f32) -> Value {
    let mut graph = FunctionGraph::default();
    let number = graph.add("get_float1", &[], Some(Value::Text("$number".into())));
    let zero = graph.add_constant(Value::Float(0.0));
    let result = match slot {
        0 => select_iteration(&mut graph, number, markers[0], negative, zero),
        1 => {
            let later = select_iteration(&mut graph, number, markers[1], positive, zero);
            select_iteration(&mut graph, number, markers[0], negative, later)
        }
        3 => select_iteration(&mut graph, number, markers[2], positive, zero),
        _ => zero,
    };
    graph.output = Some(result);
    Value::Function(Box::new(graph))
}

fn active_iterations(iterations: &[i32]) -> Value {
    let mut graph = FunctionGraph::default();
    let number = graph.add("get_float1", &[], Some(Value::Text("$number".into())));
    let mut selected = graph.add_constant(Value::Bool(false));
    for &iteration in iterations {
        let value = graph.add_constant(Value::Float(iteration as f32));
        let matches = graph.add("eq", &[number, value], None);
        selected = graph.add("or", &[selected, matches], None);
    }
    graph.output = Some(selected);
    Value::Function(Box::new(graph))
}

#[test]
fn deep_repeated_quadrants_submit_first_level_shapes_by_slot_then_iteration() {
    let definition = find("fx_map").unwrap();
    let tiny = f32::EPSILON * 0.25;
    let sparse = [2, 6];
    let four = [0, 3, 5, 8];
    for (depth, repeats, active) in [(5, 2, &[][..]), (6, 2, &[][..]), (10, 4, &[][..]), (5, 9, &sparse), (6, 9, &sparse), (9, 9, &[0, 1]), (10, 9, &four)] {
        let markers = if active.is_empty() { [0, 1, repeats - 1] } else { [active[0], active[1], active[active.len() - 1]] };
        for reversed in [false, true] {
            let (negative, positive) = if reversed { (tiny, -1.0) } else { (-1.0, tiny) };
            let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
            let first = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(repeats));
            let branch = if active.is_empty() {
                first
            } else {
                let switch = graph.add(PatternNodeKind::Switch).unwrap();
                let node = &mut graph.nodes[(switch - 1) as usize];
                node.children[1] = Some(first);
                node.parameters.insert("switch".into(), active_iterations(active));
                switch
            };
            graph.nodes[0].children[1] = Some(branch);
            let mut next = None;
            for _ in 2..=depth {
                let id = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(id - 1) as usize].children[0] = next;
                next = Some(id);
            }
            for (slot, offset) in [[0.0, 0.0], [0.0, 0.0], [0.5, -0.5], [-0.5, -0.5]].into_iter().enumerate() {
                let id = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(first - 1) as usize].children[slot] = Some(id);
                let node = &mut graph.nodes[(id - 1) as usize];
                node.children[0] = next;
                node.parameters = additive_square(0.0, [0.0; 2], offset);
                node.parameters.insert("patternsize".into(), Value::Float2([0.125; 2]));
                node.parameters.insert("opacity".into(), field(slot, markers, negative, positive));
            }
            let params = resolve_params(
                definition,
                &BTreeMap::from([
                    ("pattern".into(), Value::PatternGraph(Box::new(graph))),
                    ("roughness".into(), Value::Float(0.0)),
                    ("color_mode".into(), Value::Bool(false)),
                    ("background".into(), Value::Color([1.0; 4])),
                ]),
            );
            let vars = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
            let context = CookContext {
                w: 32,
                h: 32,
                seed: 200153,
                format: OutputFormat::F32,
                tile: false,
                inputs: &[],
                input_formats: &[],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let output = (definition.cook)(&context).unwrap().remove(0);
            let left = if (depth < 6) != reversed { tiny } else { 0.0 };
            let right = if reversed { 0.0 } else { tiny };
            assert_eq!([output.get_gray(8, 8, false), output.get_gray(24, 8, false)], [left, right], "depth {depth}, repeats {repeats}, reversed {reversed}");
        }
    }
}

#[test]
fn repeated_six_level_trees_share_quadrant_batches() {
    let tiny = f32::EPSILON * 0.25;
    for (repetitions, negative, positive, first_leaves) in [(2, (0, 0), (3, 1), 0), (5, (0, 4), (1, 0), 272)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
        let root = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[1] = Some(root);
        graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(repetitions));
        for slot in 0..4 {
            let mut node = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(root - 1) as usize].children[slot] = Some(node);
            for _ in 1..5 {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(node - 1) as usize].children.fill(Some(child));
                node = child;
            }
            let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(node - 1) as usize].children.fill(Some(leaf));
            let left = -0.5 * (slot & 1) as f32;
            let top = -0.5 * (slot >> 1) as f32;
            let neutral = [left, top + 0.5];
            graph.nodes[(node - 1) as usize].parameters = additive_square(0.25, [0.0; 2], neutral);
            let mut params = additive_square(0.25, [0.0; 2], neutral);
            let marker = [(negative, -1.0), (positive, tiny)].into_iter().find(|((quadrant, _), _)| *quadrant == slot);
            if let Some(((_, repetition), gain)) = marker {
                // Only these two leaves overlap in the upper-left quarter. Other draws still fill the batches.
                for (name, yes, no) in
                    [("frameoffset", Value::Float2([left, top]), Value::Float2(neutral)), ("opacity", Value::Float(gain), Value::Float(0.25))]
                {
                    let mut field = FunctionGraph::default();
                    let number = field.add("get_float1", &[], Some(Value::Text("$number".into())));
                    let which = field.add_constant(Value::Float(repetition as f32));
                    let active = field.add("eq", &[number, which], None);
                    let yes = field.add_constant(yes);
                    let no = field.add_constant(no);
                    field.output = Some(field.add("ifelse", &[active, yes, no], None));
                    params.insert(name.into(), Value::Function(Box::new(field)));
                }
            }
            graph.nodes[(leaf - 1) as usize].parameters = params;
        }
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
        let image = cook_in_regions(graph, &variables, 310373, [64; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        for y in 0..32 {
            for x in 0..32 {
                let leaf = (0..5).fold(0, |index, bit| index | ((x >> bit) & 1) << (2 * bit) | ((y >> bit) & 1) << (2 * bit + 1));
                let expected = if leaf < first_leaves { tiny } else { 0.0 };
                assert_eq!(image.get_gray(x, y, false).to_bits(), expected.to_bits(), "{repetitions} trees at ({x},{y})");
            }
        }
    }
}
