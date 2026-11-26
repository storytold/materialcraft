//! The FX-Map editor's node kind.

use craft_behavior::IconId;
use craft_ui::graph::{NodeHeader, NodeKind, NodeShape, PortDirection, PortSide};
use craft_ui::primitives::{Draw, IconSpec, PaintShape};
use craft_ui::style::{DataColor, StyleCx, rgba_color};
use egui::{CornerRadius, Pos2, Rect, Vec2, pos2, vec2};
use materialcraft_engine::core::{PatternNode, PatternNodeKind};

use crate::graph::node_colors::NodeColors;
use crate::i18n::tr;

/// The side of an FX-Map card, in graph units.
const NODE_SIZE: f32 = 96.0;
/// The side of a Dot, centred on its stored position.
const DOT_SIZE: f32 = 20.0;
/// The height of a card's header band.
const HEADER_HEIGHT: f32 = 18.0;
/// The side of the kind symbol in the body.
const SYMBOL_SIZE: f32 = 48.0;
/// The side of the root's star and its inset from the body's top end corner.
const STAR_SIZE: f32 = 11.0;
const STAR_INSET: f32 = 4.0;

/// An FX-Map node as the framework node graph draws it: input on top, outputs on the bottom, or a Dot.
///
/// Port 0 is the input; port `1 + slot` is child slot `slot`.
pub(crate) struct FxMapKind {
    kind: PatternNodeKind,
    children: usize,
    root: bool,
    title: String,
}

impl FxMapKind {
    /// The kind of `node`; `root` marks the FX-Map's root node with a star.
    pub(crate) fn new(node: &PatternNode, root: bool) -> Self {
        FxMapKind { kind: node.kind, children: node.children.len(), root, title: tr(node.kind.label()).to_owned() }
    }

    /// The slot of the port with this index; `None` for the input.
    pub(crate) fn port_slot(&self, port: usize) -> Option<usize> {
        port.checked_sub(1)
    }

    /// The published id of the port with this index: `input`, `output0..` or `output`.
    pub(crate) fn port_id(&self, port: usize) -> String {
        match self.port_slot(port) {
            None => "input".to_owned(),
            Some(slot) => self.kind.child_port(slot).unwrap_or_default().to_owned(),
        }
    }

    /// The 96 x 96 card or the 20 x 20 Dot at `pos`.
    pub(crate) fn rect(&self, pos: [f32; 2]) -> Rect {
        let [x, y] = materialcraft_engine::graph_grid::snap_position(pos, false);
        if self.kind == PatternNodeKind::Dot {
            Rect::from_center_size(pos2(x, y), Vec2::splat(DOT_SIZE))
        } else {
            Rect::from_min_size(pos2(x, y), Vec2::splat(NODE_SIZE))
        }
    }

    fn colors(cx: &StyleCx<'_>) -> NodeColors {
        cx.token_group::<NodeColors>()
    }
}

/// The name of child slot `slot` of `kind`.
fn slot_label(kind: PatternNodeKind, slot: usize) -> &'static str {
    let labels: &[&str] = match kind {
        PatternNodeKind::Quadrant => &["Top left", "Top right", "Bottom left", "Bottom right"],
        PatternNodeKind::Switch => &["False", "True"],
        PatternNodeKind::Iterate => &["Once", "Repeat"],
        PatternNodeKind::Dot => &["Output"],
    };
    labels.get(slot).copied().unwrap_or("Child")
}

/// The icon of the kind's symbol in the card body.
fn symbol(kind: PatternNodeKind) -> Option<IconId> {
    match kind {
        PatternNodeKind::Quadrant => Some(IconId::new("grid")),
        PatternNodeKind::Switch => Some(IconId::new("split")),
        PatternNodeKind::Iterate => Some(IconId::new("refresh")),
        PatternNodeKind::Dot => None,
    }
}

fn data(color: craft_theme::Rgba) -> DataColor {
    DataColor::new(rgba_color(color))
}

impl NodeKind<()> for FxMapKind {
    fn title(&self) -> &str {
        &self.title
    }

    fn port_count(&self) -> usize {
        1 + self.children
    }

    fn port_name(&self, port: usize) -> String {
        tr(self.port_slot(port).map_or("Input", |slot| slot_label(self.kind, slot))).to_owned()
    }

    fn port_offset(&self, port: usize, node_size: Vec2) -> Vec2 {
        match self.port_slot(port) {
            None => vec2(node_size.x / 2.0, 0.0),
            Some(slot) => vec2(node_size.x * (slot as f32 + 0.5) / self.children.max(1) as f32, node_size.y),
        }
    }

    fn paint_body(&self, draw: &Draw<'_>, rect: Rect, cx: &StyleCx<'_>) {
        let Some(icon) = symbol(self.kind) else { return };
        let colors = Self::colors(cx);
        draw.data_fill(&PaintShape::Rect(rect, CornerRadius::ZERO), data(colors.fx_kind(self.kind)));
        let side = SYMBOL_SIZE.min(rect.width()).min(rect.height());
        draw.data_icon(Rect::from_center_size(rect.center(), Vec2::splat(side)), &IconSpec::svg(icon), data(colors.fx_symbol()));
        if self.root {
            let corner = Pos2::new(rect.max.x - STAR_INSET, rect.min.y + STAR_INSET);
            let star = Rect::from_min_max(Pos2::new(corner.x - STAR_SIZE, corner.y), Pos2::new(corner.x, corner.y + STAR_SIZE));
            draw.data_icon(star, &IconSpec::svg(IconId::new("star_filled")), data(colors.fx_root()));
        }
    }

    fn port_direction(&self, port: usize) -> PortDirection {
        if port == 0 { PortDirection::Input } else { PortDirection::Output }
    }

    fn port_side(&self, port: usize) -> PortSide {
        if port == 0 { PortSide::Top } else { PortSide::Bottom }
    }

    /// A node can be the child of several parents, so a drag from its input starts a reverse wire and keeps its wires.
    fn port_takes_many(&self, port: usize) -> bool {
        port == 0
    }

    fn port_tone(&self, _port: usize, cx: &StyleCx<'_>) -> Option<DataColor> {
        Some(data(Self::colors(cx).fx_port()))
    }

    fn header(&self, cx: &StyleCx<'_>) -> Option<NodeHeader> {
        (self.kind != PatternNodeKind::Dot).then(|| NodeHeader::new(data(Self::colors(cx).fx_kind(self.kind)), HEADER_HEIGHT))
    }

    fn shape(&self) -> NodeShape {
        if self.kind == PatternNodeKind::Dot { NodeShape::Dot } else { NodeShape::Card }
    }
}
