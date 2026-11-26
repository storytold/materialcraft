use craft_behavior::Selection;
use craft_behavior::canvas::CanvasView;
use craft_behavior::kurbo::Point;
use craft_ui::canvas::{ViewCommands, ViewRequest};
use egui::Pos2;
use materialcraft_engine::core::{NodeId, PatternGraph, Value};
use serde_json::json;

use crate::MaterialCraftApp;
use crate::graph::{WireRef, wire_id};

pub struct PatternEditor {
    pub node: NodeId,
    pub graph: usize,
    pub(crate) selection: Vec<u32>,
    /// The selected wires by [`wire_id`] of their `(parent, child port)` reference, in the order they were picked.
    pub(crate) selected_wires: Selection<u64>,
    pub(crate) inspected_node: Option<u32>,
    pub(super) menu_at: Pos2,
    /// The live view of the canvas.
    pub(crate) view: CanvasView,
    /// The view commands for the canvas's next pass; starts with a fit.
    pub(crate) queued: Vec<ViewRequest>,
    /// The canvas menu is open.
    pub(crate) menu_open: bool,
}

/// One wire of an FX-Map: the parent, its child slot, the child, and the reference the wire selection hashes.
pub(super) struct PatternWire {
    pub(super) parent: u32,
    pub(super) slot: usize,
    pub(super) child: u32,
    pub(super) wire: WireRef,
}

/// Every wire of `graph`: one per filled child slot, keyed by the parent and the slot's port id.
pub(super) fn wires(graph: &PatternGraph) -> Vec<PatternWire> {
    graph
        .nodes
        .iter()
        .flat_map(|node| {
            node.children.iter().enumerate().filter_map(move |(slot, child)| {
                let port = node.kind.child_port(slot)?;
                Some(PatternWire { parent: node.id, slot, child: (*child)?, wire: (NodeId(u64::from(node.id)), port.to_owned()) })
            })
        })
        .collect()
}

impl PatternEditor {
    pub(crate) fn camera(&self) -> CanvasView {
        self.view
    }

    pub(crate) fn set_camera(&mut self, view: CanvasView) {
        self.view = view;
    }

    pub fn new(graph: usize, node: NodeId) -> Self {
        Self {
            node,
            graph,
            selection: Vec::new(),
            selected_wires: Selection::default(),
            inspected_node: None,
            menu_at: Pos2::ZERO,
            // The canvas replaces the rect before the first draw; a rect with no area opens fitted.
            view: CanvasView::new(egui::Rect::NOTHING, Point::ZERO, 1.0),
            queued: vec![ViewRequest::Place(ViewCommands::FitOnScreen)],
            menu_open: false,
        }
    }

    /// Opens the canvas menu at `at`, a point of the graph.
    pub(crate) fn open_menu(&mut self, at: Pos2) {
        self.menu_at = at;
        self.menu_open = true;
    }

    /// The selected wires, as references, in the order they were picked (the refs need the graph, which the editor
    /// does not hold); an id no live wire has drops out.
    pub(crate) fn selected_wire_refs(&self, app: &MaterialCraftApp) -> Vec<WireRef> {
        let Some(graph) = self.current(app) else { return Vec::new() };
        let live = wires(graph);
        self.selected_wires.items.iter().filter_map(|id| live.iter().find(|wire| wire_id(&wire.wire) == *id)).map(|wire| wire.wire.clone()).collect()
    }

    pub(crate) fn current<'a>(&self, app: &'a MaterialCraftApp) -> Option<&'a PatternGraph> {
        if app.session.graph != self.graph {
            return None;
        }

        let node = app.session.graph().ok()?.node(self.node).ok()?;
        if node.def != "fx_map" {
            return None;
        }

        let default = materialcraft_engine::nodes::find(&node.def)?.param("pattern");
        match node.params.get("pattern").or_else(|| default.map(|parameter| &parameter.default)) {
            Some(Value::PatternGraph(graph)) => Some(graph.as_ref()),
            None => {
                static EMPTY: std::sync::OnceLock<PatternGraph> = std::sync::OnceLock::new();
                Some(EMPTY.get_or_init(PatternGraph::default))
            }
            _ => None,
        }
    }

    pub(crate) fn selection_parameters(&self, command: &str) -> serde_json::Value {
        let selection = if matches!(command, "edit.selectAll" | "edit.selectNone") { &[][..] } else { self.selection.as_slice() };
        json!({"graph": self.graph, "pattern": self.node.0, "ids": selection, "at": [self.menu_at.x, self.menu_at.y]})
    }

    pub(super) fn edit(&mut self, app: &mut MaterialCraftApp, edit: materialcraft_engine::PatternEdit, merge: bool) {
        if self.current(app).is_none() {
            return;
        }

        if let Some(result) = app.run("graph.pattern.edit", json!({"id": self.node.0, "edit": edit, "merge": merge}))
            && let Some(id) = result.get("id").and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok())
        {
            self.selection = vec![id];
            self.selected_wires = Selection::default();
        }
    }
}
