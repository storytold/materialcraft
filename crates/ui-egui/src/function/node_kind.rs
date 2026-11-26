//! The Function graph's node kind.

use craft_behavior::value::EditEvent;
use craft_ui::graph::NodeKind;
use materialcraft_engine::core::Value;

/// A Function node as the framework node graph draws it, with the inline value control above the edit zoom.
#[allow(dead_code)] // P3(graph-w9): ge.A14
pub(crate) struct FunctionKind<'a> {
    _node: std::marker::PhantomData<&'a ()>,
}

/// The edit event of a Function node's inline value control.
#[allow(dead_code)] // P3(graph-w9): ge.A14
pub(crate) struct FunctionValueEdit {
    /// The Function node.
    pub id: u32,
    /// What the control reported.
    pub edit: EditEvent<Value>,
}

impl NodeKind<FunctionValueEdit> for FunctionKind<'_> {}
