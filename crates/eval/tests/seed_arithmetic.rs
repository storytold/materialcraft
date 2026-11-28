use materialcraft_core::{BaseParameter, FunctionGraph, Graph, OutputFormat, Package, ParameterInheritance, Value, ValueType};
use materialcraft_eval::{Cache, Options, cook};
use std::sync::Mutex;

#[test]
fn graph_and_node_seeds_preserve_dynamic_scalar_words() {
    let mut graph = Graph::new("Field");
    graph.size = [4; 2];
    graph.format = OutputFormat::F32;
    graph.seed_mode = ParameterInheritance::Absolute;
    let processor = graph.add_node("pixel_processor", [0.0; 2]);
    let mut draw = FunctionGraph::default();
    let one = draw.add_constant(Value::Float(1.0));
    draw.output = Some(draw.add("rand", &[one], None));
    graph.node_mut(processor).unwrap().params.insert("function".into(), Value::Function(Box::new(draw)));
    let mut package = Package::new("Seed arithmetic");
    package.graphs.push(graph);

    let mut function = FunctionGraph::input("$size", ValueType::Float2);
    let width = function.add("swizzle1", &[function.output.unwrap()], Some(Value::Int(0)));
    let scalar_one = function.add_constant(Value::Float(1.0));
    let sum = function.add("add", &[width, scalar_one], None);
    let product = function.add("mul", &[width, scalar_one], None);
    let word = function.add("toint1", &[width], None);
    let sum_word = function.add("toint1", &[sum], None);
    let product_word = function.add("toint1", &[product], None);
    let next_word = function.add("add", &[word, word], None);
    let negative = function.add_constant(Value::Float(-1.5));
    let fixed = function.add("toint1", &[negative], None);
    let width_word = 16.0_f32.to_bits();
    let cache = Mutex::new(Cache::default());
    for (result, expected) in
        [(word, width_word), (sum_word, (-1.0_f32).to_bits()), (product_word, width_word), (next_word, width_word.wrapping_mul(2)), (fixed, (-2_i32) as u32)]
    {
        function.output = Some(result);
        for on_node in [false, true] {
            let mut driven = package.clone();
            let mut literal = package.clone();
            if on_node {
                let base = &mut driven.graphs[0].node_mut(processor).unwrap().base;
                base.seed_mode = ParameterInheritance::Absolute;
                base.functions.insert(BaseParameter::RandomSeed, function.clone());
                let base = &mut literal.graphs[0].node_mut(processor).unwrap().base;
                base.seed_mode = ParameterInheritance::Absolute;
                base.seed = expected as i32;
            } else {
                driven.graphs[0].seed_function = Some(function.clone());
                literal.graphs[0].seed = expected as i32;
            }
            let options = Options::new(&|_| None);
            let actual = cook(&driven, 0, &options, &cache);
            let control = cook(&literal, 0, &options, &cache);
            assert_eq!(actual.nodes[&processor].error, None);
            assert_eq!(actual.nodes[&processor].base.seed, expected, "output {result}, on node {on_node}");
            assert_eq!(actual.image(processor, 0).unwrap().data(), control.image(processor, 0).unwrap().data());
        }
    }
}
