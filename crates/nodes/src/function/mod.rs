//! Native function primitives, compilation and bounded evaluation

// Individual graphs stay capped at 4096 nodes
// Shared calls can expand to 72,973 nodes, so retain a global ceiling
const MAX_EXPANDED_NODES: usize = 131_072;
const MAX_CALL_DEPTH: usize = 16;

// Keep absent loop limits aligned with fresh-node defaults
const DEFAULT_LOOP_LIMIT: i32 = 32;

mod arithmetic;
mod assigned_product;
mod binary_exponential;
mod branch_assignments;
mod branch_inputs;
mod catalog;
mod compile;
mod evaluation_context;
mod evaluation_state;
mod evaluation_step;
mod fixed_point;
mod function_call;
mod function_definition;
mod function_scope;
mod inferred_type;
mod instruction;
mod loop_sum;
mod node_interface;
mod node_types;
mod operation;
mod output;
mod pattern_channel_operations;
mod pattern_expression;
mod pixel_condition;
mod processor;
mod product_sum;
mod program;
mod reciprocal;
mod resolve_instances;
mod returned_product;
mod sequence_prefix;
mod square_root;
mod system_variable;
mod trigonometry;
mod unconnected_input;
#[cfg(test)]
#[path = "unconnected.test.rs"]
mod unconnected_tests;
pub mod value_type;
mod vector;
mod warnings;

pub(crate) use arithmetic::flush_subnormal;
pub(crate) use binary_exponential::{exp2 as pixel_exp2, signed_pixel_power};
pub use catalog::{FUNCTIONS, default_value, definition};
pub use compile::{compile, compile_with_literal_inputs};
pub use evaluation_context::EvaluationContext;
pub use evaluation_state::EvaluationState;
pub(crate) use fixed_point::truncate_float;
pub use function_definition::FunctionDefinition;
pub use function_scope::FunctionScope;
pub use node_interface::{graph_ports, input_identifiers, node_label};
pub use node_types::node_types;
pub use output::{can_be_output, output_node};
pub(crate) use pattern_expression::PatternExpression;
pub(crate) use processor::pixel_processor;
pub use processor::pixel_processor_with_dependencies;
pub use program::Program;
pub(crate) use reciprocal::divide as pixel_divide;
pub use resolve_instances::resolve_instances;
pub(crate) use square_root::evaluate as pixel_sqrt;
pub use system_variable::SystemVariable;
pub(crate) use trigonometry::{pixel_sin_cos, uniform_sin_cos_turns};
pub use warnings::warnings;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "system_variable.test.rs"]
mod system_variable_tests;
