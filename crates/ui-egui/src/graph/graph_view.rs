use super::{NodeSearch, PortHit, WireRef};
use craft_behavior::Selection;
use craft_ui::canvas::ViewRequest;
use egui::{Pos2, Rect, Vec2};
use materialcraft_engine::core::NodeId;
use std::hash::{DefaultHasher, Hash, Hasher};

pub struct GraphView {
    pub(crate) inspected_node: Option<NodeId>,
    pub(super) thumbnails_cleared: bool,
    pub(crate) search: Option<NodeSearch>,
    /// Node rectangles on screen last frame (for `ui.inspect` and scripted input)
    pub rects: Vec<(NodeId, Rect)>,
    /// Ports on screen last frame, each node's in its kind's port order
    pub ports: Vec<PortHit>,
    pub frames: Vec<(u64, Rect)>,
    pub comments: Vec<(u64, Rect)>,
    pub pins: Vec<(u64, Rect)>,
    pub(crate) annotation_target: materialcraft_engine::AnnotationTarget,
    pub(crate) pin_description_focus: Option<u64>,
    /// The canvas rectangle last frame
    pub view: Rect,
    /// The material canvas's rectangle when it last showed: the rect its saved camera is anchored in. `view` holds a
    /// Function or FX-Map editor's rect while one is open, and the canvas keeps the centre when its rect changes.
    pub(super) camera_rect: Rect,
    /// Graph point of the last right-click (context menu actions add things there)
    pub(super) menu_at: Pos2,
    /// The graph the camera last framed: no framework signal says the session's graph changed, so the app keeps
    /// which graph its fit was for, and a different one queues a fit
    pub(super) framed_graph: Option<usize>,
    /// The selected wires by [`wire_id`], in the order they were picked (Delete removes them, X swaps them)
    pub selected_wires: Selection<u64>,
    /// The wire the context menu is for (right-clicked on a wire)
    pub(super) menu_wire: Option<WireRef>,
    /// Wires on screen last frame: the wire and its bezier control points
    pub wires: Vec<(WireRef, [Pos2; 4])>,
    /// The frame being moved and its offset
    pub(crate) moving: Option<(craft_ui::graph::AnnotationKey, Vec2)>,
    /// The node-move preview offset, in graph units: set while the selected nodes are dragged, cleared on commit or
    /// cancel
    pub(crate) node_move: Option<Vec2>,
    /// The backdrop resize preview: the frame and its rect
    pub(crate) resizing: Option<(u64, Rect)>,
    /// View commands for the canvas's next pass
    pub(super) pending_views: Vec<ViewRequest>,
}

/// The frames', comments' and pins' ids and rects on screen, in that order.
pub(crate) type AnnotationRects = (Vec<(u64, Rect)>, Vec<(u64, Rect)>, Vec<(u64, Rect)>);

/// The reference of a wire for the framework's wire selection: a hash of the input it feeds, which is the wire's
/// identity (an input takes one wire), so a wire keeps its id while others come and go.
pub fn wire_id(wire: &WireRef) -> u64 {
    let mut hasher = DefaultHasher::new();
    wire.0.0.hash(&mut hasher);
    wire.1.hash(&mut hasher);
    hasher.finish()
}

impl GraphView {
    /// The wires selected, as references, in the order they were picked; ids not on screen last frame drop out.
    pub(crate) fn selected_wire_refs(&self) -> Vec<WireRef> {
        self.selected_wires.items.iter().filter_map(|id| self.wires.iter().find(|(wire, _)| wire_id(wire) == *id)).map(|(wire, _)| wire.clone()).collect()
    }

    /// Queues a view command for the canvas's next pass.
    pub(crate) fn request_view(&mut self, request: ViewRequest) {
        self.pending_views.push(request);
    }

    /// Stores what the canvas drew this pass: the view, the node, port and wire rects, and the frames', comments' and
    /// pins' rects as `annotations::screen_rects` returns them.
    // PLAN: ge.A2 lists frames, comments and pins as three parameters (eight with `self`, over clippy's
    // too_many_arguments); they come as the one tuple `screen_rects` returns, so graph-w9 and graph-w15 pass that.
    pub(crate) fn publish(
        &mut self,
        view: Rect,
        rects: Vec<(NodeId, Rect)>,
        ports: Vec<PortHit>,
        wires: Vec<(WireRef, [Pos2; 4])>,
        (frames, comments, pins): AnnotationRects,
    ) {
        self.view = view;
        self.rects = rects;
        self.ports = ports;
        self.wires = wires;
        self.frames = frames;
        self.comments = comments;
        self.pins = pins;
    }

    pub(crate) fn cancel_drag(&mut self) {
        self.node_move = None;
        self.moving = None;
        self.resizing = None;
        self.search = None;
        self.menu_wire = None;
    }

    pub(crate) fn cancel_gesture(&mut self) {
        self.cancel_drag();
        self.framed_graph = None;
    }
}

impl Default for GraphView {
    fn default() -> Self {
        GraphView {
            inspected_node: None,
            thumbnails_cleared: false,
            search: None,
            rects: Vec::new(),
            ports: Vec::new(),
            frames: Vec::new(),
            comments: Vec::new(),
            pins: Vec::new(),
            annotation_target: Default::default(),
            pin_description_focus: None,
            view: Rect::NOTHING,
            camera_rect: Rect::NOTHING,
            menu_at: Pos2::ZERO,
            framed_graph: None,
            selected_wires: Selection::default(),
            menu_wire: None,
            wires: Vec::new(),
            moving: None,
            node_move: None,
            resizing: None,
            pending_views: Vec::new(),
        }
    }
}
