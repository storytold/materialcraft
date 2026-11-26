use crate::MaterialCraftApp;
use crate::graph::WireRef;
use craft_behavior::Selection;
use craft_behavior::canvas::CanvasView;
use craft_ui::canvas::{ViewCommands, ViewRequest};
use egui::Pos2;
use materialcraft_engine::core::FunctionGraph;
use materialcraft_engine::core::NodeId;
use serde_json::json;

use super::FunctionTarget;

pub struct FunctionEditor {
    pub node: Option<NodeId>,
    pub graph: usize,
    pub(crate) selection: Vec<u32>,
    /// The selected wires by `wire_id`, in the order they were picked.
    pub(crate) selected_wires: Selection<u64>,
    pub(crate) inspected_node: Option<u32>,
    pub target: FunctionTarget,
    pub(super) menu_at: Pos2,
    pub(super) open_reference: Option<u32>,
    /// The live view of the canvas
    pub(crate) view: CanvasView,
    /// The view commands for the canvas's next pass; starts with a fit
    pub(crate) queued: Vec<ViewRequest>,
}

/// One wire of a Function graph: the source node, the fed node, its input and the wire's reference.
pub(super) struct FunctionWire {
    pub(super) from: u32,
    pub(super) to: u32,
    pub(super) input: usize,
    pub(super) wire: WireRef,
}

/// The wires of `graph`, one per fed input, in node order. The wire's id is `wire_id(&wire)`, the hash the material
/// editor uses, so the canvas, the selection and the menus name a wire alike.
pub(super) fn wires(graph: &FunctionGraph) -> impl Iterator<Item = FunctionWire> + '_ {
    graph.nodes.iter().flat_map(|node| {
        node.inputs.iter().enumerate().filter_map(move |(input, source)| {
            source.map(|from| FunctionWire { from, to: node.id, input, wire: (NodeId(u64::from(node.id)), input.to_string()) })
        })
    })
}

impl FunctionEditor {
    /// The selected wires, as references, in the order they were picked; a wire no longer in the graph is left out.
    pub(crate) fn selected_wire_refs(&self, app: &MaterialCraftApp) -> Vec<WireRef> {
        let Some(graph) = self.current(app) else { return Vec::new() };
        let live: Vec<(u64, WireRef)> = wires(graph).map(|wire| (crate::graph::wire_id(&wire.wire), wire.wire)).collect();
        self.selected_wires.items.iter().filter_map(|id| live.iter().find(|(live, _)| live == id).map(|(_, wire)| wire.clone())).collect()
    }

    pub fn standalone(graph: usize) -> Self {
        Self::with_target(graph, None, FunctionTarget::Graph)
    }

    /// The live view of the canvas.
    pub(crate) fn camera(&self) -> CanvasView {
        self.view
    }

    /// Shows `view` from the canvas's next pass on.
    pub(crate) fn set_camera(&mut self, view: CanvasView) {
        self.view = view;
    }

    pub fn new(graph: usize, node: NodeId, target: FunctionTarget) -> Self {
        Self::with_target(graph, Some(node), target)
    }

    fn with_target(graph: usize, node: Option<NodeId>, target: FunctionTarget) -> Self {
        Self {
            graph,
            node,
            target,
            selection: Vec::new(),
            selected_wires: Selection::default(),
            inspected_node: None,
            menu_at: Pos2::ZERO,
            open_reference: None,
            view: CanvasView::new(egui::Rect::NOTHING, craft_behavior::kurbo::Point::ZERO, 1.0),
            queued: vec![ViewRequest::Place(ViewCommands::FitOnScreen)],
        }
    }

    pub(crate) fn current<'a>(&self, app: &'a MaterialCraftApp) -> Option<&'a FunctionGraph> {
        if self.graph != app.session.graph {
            return None;
        }

        self.command_target()?.current(&app.session).ok()
    }

    pub(crate) fn command_target(&self) -> Option<materialcraft_engine::FunctionGraphTarget> {
        use materialcraft_engine::FunctionGraphTarget;
        Some(match &self.target {
            FunctionTarget::Graph => FunctionGraphTarget::Graph,
            FunctionTarget::Parameter(parameter) => FunctionGraphTarget::Parameter { node: self.node?, parameter: parameter.clone() },
            FunctionTarget::Dynamic(parameter) => FunctionGraphTarget::Dynamic { node: self.node?, parameter: parameter.clone() },
            FunctionTarget::Base(parameter) => FunctionGraphTarget::Base { node: self.node?, parameter: *parameter },
            FunctionTarget::Pattern { node, parameter } => FunctionGraphTarget::Pattern { owner: self.node?, node: *node, parameter: parameter.clone() },
        })
    }

    pub(crate) fn edit(&mut self, app: &mut MaterialCraftApp, edit: materialcraft_engine::FunctionEdit, merge: bool) {
        if self.current(app).is_none() {
            return;
        }
        let Some(target) = self.command_target() else { return };

        if let Some(result) = app.run("graph.function.edit", json!({"target": target, "edit": edit, "merge": merge}))
            && let Some(id) = result.get("id").and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok())
        {
            self.selection = vec![id];
            self.selected_wires = Selection::default();
            if self.target == FunctionTarget::Graph {
                app.session.selection = vec![NodeId(u64::from(id))];
                app.session.annotation_selection = None;
            }
        }
    }

    pub(crate) fn selection_parameters(&self, command: &str) -> serde_json::Value {
        let selection = if matches!(command, "edit.selectAll" | "edit.selectNone") { &[][..] } else { self.selection.as_slice() };
        json!({"graph": self.graph, "function": self.command_target(), "ids": selection, "at": [self.menu_at.x, self.menu_at.y]})
    }
}
