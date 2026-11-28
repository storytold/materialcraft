use materialcraft_core::ValueType;
use materialcraft_nodes::PortKind;

#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub id: String,
    pub label: String,
    pub kind: PortKind,
    /// Concrete type of a declared value port.
    pub value_type: Option<ValueType>,
}
