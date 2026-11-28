use materialcraft_core::{PatternGraph, PatternNodeKind, Value};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::collections::BTreeMap;

fn render(depth: usize, roughness: f32, stack: bool, color: Option<[f32; 4]>) -> craft_texture::Image {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for index in 0..depth {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[index].children.fill(Some(child));
    }
    for (index, node) in graph.nodes.iter_mut().enumerate() {
        node.parameters.insert("patterntype".into(), Value::Int(if stack || index == depth { 2 } else { 0 }));
        node.parameters.insert("opacity".into(), Value::Float4(color.unwrap_or([1.0; 4])));
    }
    render_graph(graph, roughness, 0.125, color.is_some()).unwrap().remove(0)
}

fn render_graph(graph: PatternGraph, roughness: f32, opacity: f32, color: bool) -> materialcraft_nodes::CookResult {
    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(roughness)),
            ("opacity".into(), Value::Float(opacity)),
            ("color_mode".into(), Value::Bool(color)),
            ("background".into(), Value::Color([0.0; 4])),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 64,
        h: 64,
        seed: 0,
        tile: true,
        inputs: &[],
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };

    (definition.cook)(&context)
}

#[test]
fn finite_amplified_stamps_between_samples_do_not_exhaust_the_visit_budget() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let background = graph.add(PatternNodeKind::Quadrant).unwrap();
    let tiny = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(background), Some(tiny)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(1_048_580));
    graph.nodes[1].parameters = BTreeMap::from([("patterntype".into(), Value::Int(2)), ("opacity".into(), Value::Float(0.25))]);
    graph.nodes[2].parameters = BTreeMap::from([("patterntype".into(), Value::Int(2)), ("patternsize".into(), Value::Float2([1.0 / 256.0; 2]))]);
    let image = render_graph(graph, 0.6, 2.0, false).unwrap().remove(0);
    assert!(image.data().iter().all(|&pixel| pixel == 0.5));
}

#[test]
fn a_missed_stamp_still_validates_its_depth_weight() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    assert!(render_graph(graph.clone(), -1.0, 1.0, false).is_ok());
    graph.nodes[0].parameters = BTreeMap::from([("patterntype".into(), Value::Int(2)), ("patternsize".into(), Value::Float2([1.0 / 256.0; 2]))]);
    assert!(render_graph(graph, -1.0, 1.0, false).unwrap_err().contains("roughness must be finite"));
}

#[test]
fn quadrant_opacity_matches_measured_depth_and_stack_values() {
    // Pixel values come from an original constant-square 32-bit float diagnostic
    // Empty ancestors affect depth normalization; zero roughness bypasses it
    for (roughness, depth, expected) in [
        (0.0, 0, 0.125),
        (0.0, 1, 0.125),
        (0.0, 4, 0.125),
        (0.42, 0, 0.125),
        (0.42, 1, 0.036971834),
        (0.42, 4, 0.0022858537),
        (0.5, 0, 0.125),
        (0.5, 1, 0.041666668),
        (0.5, 4, 0.004032258),
        (1.0, 0, 0.125),
        (1.0, 1, 0.0625),
        (1.0, 4, 0.025),
    ] {
        let image = render(depth, roughness, false, None);

        assert!(
            image.data().iter().all(|value| (*value - expected).abs() < 2e-7),
            "roughness {roughness}, depth {depth}: {:?}, expected {expected}",
            image.mean()
        );
    }

    for roughness in [0.42, 1.0] {
        let image = render(4, roughness, true, None);

        assert!(image.data().iter().all(|value| (*value - 0.125).abs() < 2e-7), "stack roughness {roughness}: {:?}", image.mean());
    }
}

#[test]
fn colored_patterns_premultiply_rgb_by_their_opacity_alpha() {
    let image = render(0, 0.0, false, Some([0.4, 0.2, 0.1, 0.5]));
    for (actual, expected) in image.get(32, 32, false).into_iter().zip([0.025, 0.0125, 0.00625, 0.0625]) {
        assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
    }
}
