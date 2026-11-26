use egui::Pos2;

/// A graph port identified by its node and port name.
pub(crate) type PortKey = (materialcraft_engine::core::NodeId, String);

/// The wire direction associated with a node search.
#[derive(Clone, Debug)]
pub(crate) enum SearchWire {
    /// The new node's output feeds this input.
    From(PortKey),
    /// This output feeds the new node.
    To(PortKey),
}

/// What a node search can add.
#[derive(Clone, Debug)]
pub(crate) enum SearchPick {
    /// A node definition.
    Definition(&'static str),
    /// A Function graph node.
    Function(crate::function::library::FunctionLibraryDrag),
}

/// One row in a node search result list.
#[derive(Clone, Debug)]
pub(crate) struct SearchCandidate {
    /// The row label.
    pub label: String,
    /// The category shown at the row's end.
    pub group: String,
    /// The item added when this result is chosen.
    pub pick: SearchPick,
}

/// The state of the node search shared by graph editors.
#[derive(Clone, Debug)]
pub(super) struct NodeSearch {
    pub(crate) at: Pos2,
    pub(crate) text: String,
    pub(crate) wire: Option<SearchWire>,
    pub(crate) editor: super::editor_target::EditorTarget,
}
