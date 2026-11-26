use super::{GraphDrag, NodeSearch, PortHit, WireRef};
use egui::{Pos2, Rect};
use materialcraft_engine::core::NodeId;

pub struct GraphView {
    pub(crate) inspected_node: Option<NodeId>,
    pub(crate) keyboard_was_focused: bool,
    pub(super) thumbnails_cleared: bool,
    pub(super) drag: Option<GraphDrag>,
    pub(super) search: Option<NodeSearch>,
    /// Node rectangles on screen last frame (for `ui.inspect` and scripted input)
    pub rects: Vec<(NodeId, Rect)>,
    pub ports: Vec<PortHit>,
    pub frames: Vec<(u64, Rect)>,
    pub comments: Vec<(u64, Rect)>,
    pub pins: Vec<(u64, Rect)>,
    pub(crate) annotation_target: materialcraft_engine::AnnotationTarget,
    pub(crate) pin_description_focus: Option<u64>,
    /// The canvas rectangle last frame
    pub view: Rect,
    /// Graph point of the last right-click (context menu actions add things there)
    pub(super) menu_at: Pos2,
    /// The graph the camera last framed
    pub(super) framed_graph: Option<usize>,
    /// The selected wires, in the order they were picked (Delete removes them, X swaps them)
    pub selected_wires: Vec<WireRef>,
    /// The wire the context menu is for (right-clicked on a wire)
    pub(super) menu_wire: Option<WireRef>,
    /// Wires on screen last frame: the wire and its bezier control points
    pub wires: Vec<(WireRef, [Pos2; 4])>,
    /// The frame being moved and its offset
    #[allow(dead_code)] // P3(graph-w6): ge.A2
    pub(crate) moving: Option<(craft_ui::graph::AnnotationKey, egui::Vec2)>,
    /// The node-move preview offset
    #[allow(dead_code)] // P3(graph-w6): ge.A2
    pub(crate) node_move: Option<egui::Vec2>,
    /// The backdrop resize preview: the frame and its rect
    #[allow(dead_code)] // P3(graph-w6): ge.A2
    pub(crate) resizing: Option<(u64, Rect)>,
}

/// The reference of a wire for the framework's wire selection.
#[allow(dead_code)] // P3(graph-w6): ge.A2
pub fn wire_id(wire: &WireRef) -> u64 {
    let _ = wire;
    todo!("ge.A2")
}

#[allow(dead_code)] // P3(graph-w6): ge.A2
impl GraphView {
    /// The wires selected, as references.
    pub(crate) fn selected_wire_refs(&self) -> Vec<WireRef> {
        todo!("ge.A2")
    }

    /// Queues a view command for the canvas's next pass.
    pub(crate) fn request_view(&mut self, request: craft_ui::canvas::ViewRequest) {
        let _ = request;
        todo!("ge.A2")
    }

    /// Stores what the canvas drew this pass: the view, the node, port, wire and annotation rects.
    pub(crate) fn publish(
        &mut self,
        view: Rect,
        rects: Vec<(NodeId, Rect)>,
        ports: Vec<PortHit>,
        wires: Vec<(WireRef, [Pos2; 4])>,
        frames: Vec<(u64, Rect)>,
        comments: Vec<(u64, Rect)>,
        pins: Vec<(u64, Rect)>,
    ) {
        let _ = (view, rects, ports, wires, frames, comments, pins);
        todo!("ge.A2")
    }

    pub(crate) fn cancel_drag(&mut self) {
        self.drag = None;
        self.search = None;
        self.menu_wire = None;
    }

    pub(crate) fn cancel_gesture(&mut self) {
        self.cancel_drag();
        self.framed_graph = None;
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(crate) fn search_is_open(&self) -> bool {
        self.search.is_some()
    }
}

impl Default for GraphView {
    fn default() -> Self {
        GraphView {
            inspected_node: None,
            keyboard_was_focused: false,
            thumbnails_cleared: false,
            drag: None,
            search: None,
            rects: Vec::new(),
            ports: Vec::new(),
            frames: Vec::new(),
            comments: Vec::new(),
            pins: Vec::new(),
            annotation_target: Default::default(),
            pin_description_focus: None,
            view: Rect::NOTHING,
            menu_at: Pos2::ZERO,
            framed_graph: None,
            selected_wires: Vec::new(),
            menu_wire: None,
            wires: Vec::new(),
            moving: None,
            node_move: None,
            resizing: None,
        }
    }
}
