use materialcraft_core::{FunctionGraph, Graph, NodeId, OutputFormat, Package, ParameterInheritance, Value};
use materialcraft_eval::{Cache, Options, cook};
use std::sync::Mutex;

fn field(graph: &mut Graph, pixel_size: [f32; 2]) -> NodeId {
    let mut noise = FunctionGraph::default();
    let unit = noise.add_constant(Value::Float(1.0));
    noise.output = Some(noise.add("rand", &[unit], None));
    let id = graph.add_node("pixel_processor", [0.0; 2]);
    let node = graph.node_mut(id).unwrap();
    node.params.insert("function".into(), Value::Function(Box::new(noise)));
    node.base.pixel_size = pixel_size;
    node.base.pixel_size_mode = ParameterInheritance::Absolute;
    id
}

#[test]
fn blur_inherits_the_pixel_size_of_each_graph_output_on_cold_and_cached_cooks() {
    let mut child = Graph::new("Fields");
    let mut parent = Graph::new("Filtered fields");
    for graph in [&mut child, &mut parent] {
        graph.size = [6; 2];
        graph.format = OutputFormat::U16;
    }
    let instance = parent.add_node("graph_instance", [0.0; 2]);
    parent.node_mut(instance).unwrap().params.insert("graph".into(), Value::Text("Fields".into()));
    let mut comparisons = Vec::new();
    for (identifier, size) in [("horizontal", [2.0, 0.5]), ("vertical", [0.5, 2.0])] {
        let inside = field(&mut child, size);
        let output = child.add_node("output", [0.0; 2]);
        child.node_mut(output).unwrap().params.insert("identifier".into(), Value::Text(identifier.into()));
        child.connect(inside, "output", output, "input").unwrap();
        let outside = field(&mut parent, size);
        let [nested, flat] = [(instance, identifier), (outside, "output")].map(|(upstream, port)| {
            let blur = parent.add_node("blur", [0.0; 2]);
            parent.node_mut(blur).unwrap().params.insert("intensity".into(), Value::Float(4.0));
            parent.connect(upstream, port, blur, "input").unwrap();
            blur
        });
        comparisons.push((nested, flat, size));
    }
    let mut package = Package::new("Pixel sizes");
    package.graphs.extend([parent, child]);
    let cache = Mutex::new(Cache::default());
    for cached in [false, true] {
        let cooked = cook(&package, 0, &Options::new(&|_| None), &cache);
        assert_eq!(cooked.nodes[&instance].error, None);
        assert_eq!(cooked.nodes[&instance].cached, cached);
        for &(nested, flat, size) in &comparisons {
            assert_eq!(cooked.nodes[&nested].base.pixel_size, size);
            assert_eq!(cooked.image(nested, 0).unwrap().data(), cooked.image(flat, 0).unwrap().data());
        }
    }
}
