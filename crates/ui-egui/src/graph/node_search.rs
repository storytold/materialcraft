use egui::Pos2;
use materialcraft_engine::core::NodeId;

/// The wire a node search connects the new node to.
#[allow(dead_code)] // P3(graph-w11): ge.A10
pub(crate) enum SearchWire {
    /// The new node's output feeds this input.
    From(super::search::PortKey),
    /// This output feeds the new node.
    To(super::search::PortKey),
}

/// What a node search can add.
#[allow(dead_code)] // P3(graph-w11): ge.A10
pub(crate) enum SearchPick {
    /// A node definition.
    Definition(&'static str),
    /// A Function graph node.
    Function(crate::function::library::FunctionLibraryDrag),
}

/// One row of the node search's list.
#[allow(dead_code)] // P3(graph-w11): ge.A10
pub(crate) struct SearchCandidate {
    /// The row's text.
    pub label: String,
    /// The group the row belongs to.
    pub group: String,
    /// What picking the row adds.
    pub pick: SearchPick,
}

/// The node search popup (Space or Tab)
#[derive(Clone, Debug)]
pub(super) struct NodeSearch {
    pub(super) at: Pos2,
    pub(super) text: String,
    pub(super) sel: usize,
    pub(super) connect: Option<(NodeId, String)>,
    /// Opened from this input (a wire dragged off it): the new node's output feeds it
    pub(super) connect_to: Option<(NodeId, String)>,
    pub(super) focus: bool,
}
