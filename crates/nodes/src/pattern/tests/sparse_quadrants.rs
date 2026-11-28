use super::*;
use materialcraft_core::OutputFormat;

fn at_center(weight: f32, center: [f32; 2]) -> BTreeMap<String, Value> {
    let mut frame = FunctionGraph::default();
    let target = frame.add_constant(Value::Float2(center));
    let position = frame.add("get_float2", &[], Some(Value::Text("$pos".into())));
    frame.output = Some(frame.add("sub", &[target, position], None));
    let mut params = additive_square(weight, [0.0; 2], [0.0; 2]);
    params.insert("frameoffset".into(), Value::Function(Box::new(frame)));
    params
}

fn leaf_luminosity(quotas: [f32; 3]) -> Value {
    let mut graph = FunctionGraph::default();
    let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let scale = graph.add_constant(Value::Float(16.0));
    let xy = [0, 1].map(|axis| graph.add("swizzle1", &[pos], Some(Value::Int(axis))));
    let cell = xy.map(|component| {
        let wide = graph.add("mul", &[component, scale], None);
        let floor = graph.add("floor", &[wide], None);
        let fraction = graph.add("sub", &[wide, floor], None);
        let local = graph.add("mul", &[fraction, scale], None);
        graph.add("floor", &[local], None)
    });
    let row = graph.add("mul", &[cell[1], scale], None);
    let pixel = graph.add("add", &[row, cell[0]], None);
    let [ordinary, top, middle] = quotas.map(|value| graph.add_constant(Value::Float(value)));
    let half = graph.add_constant(Value::Float(0.5));
    let quarter = graph.add_constant(Value::Float(0.25));
    let left = graph.add("lr", &[xy[0], half], None);
    let upper_half = graph.add("lr", &[xy[1], half], None);
    let upper_quarter = graph.add("lr", &[xy[1], quarter], None);
    let middle_or_lower = graph.add("ifelse", &[upper_half, middle, ordinary], None);
    let left_quota = graph.add("ifelse", &[upper_quarter, top, middle_or_lower], None);
    let quota = graph.add("ifelse", &[left, left_quota, ordinary], None);
    let active = graph.add("lr", &[pixel, quota], None);
    let on = graph.add_constant(Value::Float(0.25));
    let off = graph.add_constant(Value::Float(0.0));
    graph.output = Some(graph.add("ifelse", &[active, on, off], None));
    Value::Function(Box::new(graph))
}

fn quadrant(graph: &mut PatternGraph, parameters: BTreeMap<String, Value>, children: [Option<u32>; 4]) -> u32 {
    let id = graph.add(PatternNodeKind::Quadrant).unwrap();
    let node = &mut graph.nodes[(id - 1) as usize];
    node.parameters = parameters;
    node.children = children.to_vec();
    id
}

#[test]
fn sparse_quadrants_count_only_nonzero_copies_when_ordering_parent_and_leaf_draws() {
    let tiny = f32::EPSILON * 0.25;
    // Groups 11 and 27 have the same position within their root quadrant.
    for (quotas, group, parent_first) in
        [([86.0; 3], 27, false), ([96.0, 66.0, 96.0], 11, false), ([96.0, 96.0, 85.0], 11, true), ([96.0, 96.0, 85.0], 27, false)]
    {
        for (parent_gain, leaf_gain, reversed) in [(-1.0, tiny, false), (tiny, -1.0, true)] {
            let expected = if parent_first != reversed { tiny } else { 0.0 };
            let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
            let neutral = at_center(0.25, [0.5, 0.75]);
            let mut leaves = neutral.clone();
            leaves.insert("opacity".into(), leaf_luminosity(quotas));
            let leaf = quadrant(&mut graph, leaves, [None; 4]);
            let mut ordinary = quadrant(&mut graph, neutral.clone(), [Some(leaf); 4]);
            let column = if group == 11 { 1.0 } else { 5.0 };
            let marker = additive_square(leaf_gain, [0.0; 2], [-column / 8.0, -3.0 / 8.0]);
            let mut marked = quadrant(&mut graph, marker, [Some(leaf); 4]);
            for _ in 3..7 {
                ordinary = quadrant(&mut graph, neutral.clone(), [Some(ordinary); 4]);
                marked = quadrant(&mut graph, neutral.clone(), [Some(marked); 4]);
            }
            let parent_children = [Some(ordinary); 4];
            let parent = quadrant(&mut graph, neutral.clone(), parent_children);
            let mut marker = at_center(parent_gain, [1.0 / 16.0; 2]);
            marker.insert("patternsize".into(), Value::Float2([0.5; 2]));
            let first_parent = quadrant(&mut graph, marker, parent_children);
            let mut children = parent_children;
            children[3] = Some(marked);
            let marked_parent = quadrant(&mut graph, neutral, children);

            let common = quadrant(&mut graph, BTreeMap::new(), [Some(parent); 4]);
            let mut first_children = [Some(first_parent), Some(parent), Some(parent), Some(parent)];
            let mut second_children = [Some(parent); 4];
            if group == 11 {
                first_children[2] = Some(marked_parent);
            } else {
                second_children[2] = Some(marked_parent);
            }
            let first = quadrant(&mut graph, BTreeMap::new(), first_children);
            let second = quadrant(&mut graph, BTreeMap::new(), second_children);
            graph.nodes[0].children = vec![Some(first), Some(second), Some(common), Some(common)];
            let definition = find("fx_map").unwrap();
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
                w: 128,
                h: 128,
                seed: 189319,
                format: OutputFormat::F32,
                tile: false,
                inputs: &[],
                input_formats: &[],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let output = (definition.cook)(&context).unwrap().remove(0);
            for y in 0..16 {
                for x in 0..16 {
                    assert_eq!(output.get_gray(x, y, false), expected, "at ({x}, {y}), group {group}, quotas {quotas:?}, parent gain {parent_gain}");
                }
            }
        }
    }
}
