//! Material graph node kinds and their data colours.

use craft_behavior::IconId;
use craft_ui::{
    graph::{NodeFooter, NodeHeader, NodeKind, NodeShape, PortDirection},
    primitives::{Draw, IconSpec, Priority, SurfaceKind, TextRun, ThumbRequest},
    style::{Corners, DataColor, StyleCx},
};
use materialcraft_engine::{core::Node, eval::Port, nodes::PortKind};

use crate::{
    MaterialCraftApp,
    graph::node_colors::NodeColors,
    thumbs::{NodeFace, NodeThumb},
};

/// The width of a node card.
pub(crate) const NODE_W: f32 = 100.0;
pub(crate) const HEADER_HEIGHT: f32 = 18.0;
pub(crate) const FOOTER_HEIGHT: f32 = 14.0;
pub(crate) const PORT_GAP: f32 = 18.0;
const PORT_PADDING: f32 = 8.0;
pub(crate) const DOT_DIAMETER: f32 = 14.0;
pub(crate) const BARE_BODY_HEIGHT: f32 = 26.0;
pub(crate) const CARD_BODY_HEIGHT: f32 = 100.0;

/// A material graph node rendered by the framework graph.
pub(crate) struct MaterialKind {
    title: String,
    def: String,
    ins: Vec<Port>,
    outs: Vec<Port>,
    body_height: f32,
    footer: Option<String>,
    tooltip: Option<String>,
    texture: Option<(egui::TextureId, egui::Rect)>,
    value: Option<String>,
    status: Option<String>,
    dot: bool,
    disabled: bool,
}

impl NodeKind<()> for MaterialKind {
    fn title(&self) -> &str {
        &self.title
    }
    fn port_count(&self) -> usize {
        self.ins.len() + self.outs.len()
    }
    fn port_name(&self, port: usize) -> String {
        self.ins.get(port).or_else(|| self.outs.get(port.saturating_sub(self.ins.len()))).map_or_else(String::new, |p| p.label.clone())
    }
    fn port_direction(&self, port: usize) -> PortDirection {
        if port < self.ins.len() { PortDirection::Input } else { PortDirection::Output }
    }
    fn port_offset(&self, port: usize, size: egui::Vec2) -> egui::Vec2 {
        let input = port < self.ins.len();
        let (count, index) = if input { (self.ins.len(), port) } else { (self.outs.len(), port - self.ins.len()) };
        let y = if self.dot {
            size.y * 0.5
        } else {
            HEADER_HEIGHT + (size.y - HEADER_HEIGHT - FOOTER_HEIGHT - count as f32 * PORT_GAP) * 0.5 + PORT_GAP * 0.5 + index as f32 * PORT_GAP
        };
        egui::vec2(if input { 0.0 } else { size.x }, y)
    }
    fn port_tone(&self, port: usize, cx: &StyleCx<'_>) -> Option<DataColor> {
        let kind = self.ins.get(port).or_else(|| self.outs.get(port.saturating_sub(self.ins.len())))?.kind;
        Some(DataColor::new(craft_ui::style::rgba_color(cx.token_group::<NodeColors>().port(kind))))
    }
    fn header(&self, cx: &StyleCx<'_>) -> Option<NodeHeader> {
        if self.dot {
            return None;
        }
        let mut header = NodeHeader::new(DataColor::new(craft_ui::style::rgba_color(cx.token_group::<NodeColors>().header(&self.def))), 18.0);
        if self.def == "output" {
            header = header.icon(IconSpec::svg(IconId::new("sign-out")));
        }
        Some(header)
    }
    fn footer(&self) -> Option<NodeFooter<'_>> {
        self.footer.as_deref().map(|text| NodeFooter::new(text, 14.0))
    }
    fn shape(&self) -> NodeShape {
        if self.dot { NodeShape::Dot } else { NodeShape::Card }
    }
    fn tooltip(&self) -> Option<String> {
        self.tooltip.clone()
    }
    fn paint_body(&self, draw: &Draw<'_>, rect: egui::Rect, cx: &StyleCx<'_>) {
        let image_inset = cx.tokens.space.gap.tight;
        let text_inset = cx.tokens.space.gap.inline;
        if let Some(texture) = self.texture {
            let (id, uv) = texture;
            draw.surface(rect.shrink(image_inset), SurfaceKind::Well, Corners::All);
            if self.disabled {
                draw.faded(1.0 - craft_theme::Opacity::DISABLED, |draw| draw.texture(rect.shrink(image_inset * 2.0), id, uv));
            } else {
                draw.texture(rect.shrink(image_inset * 2.0), id, uv);
            }
        } else if let Some(value) = &self.value {
            draw.surface(rect.shrink(text_inset), SurfaceKind::Well, Corners::All);
            draw.text(rect.shrink(text_inset * 2.0), &TextRun::new(value.clone()));
        } else if let Some(status) = &self.status {
            let icon = if status == "warning" { "warning" } else { "hourglass-medium" };
            draw.icon(rect.shrink(text_inset), &IconSpec::svg(IconId::new(icon)));
        }
    }
}

impl MaterialKind {
    /// Build the graph face from a node and the evaluated port lists.
    pub(crate) fn new(app: &MaterialCraftApp, ctx: &egui::Context, node: &Node, ins: Vec<Port>, outs: Vec<Port>) -> Self {
        let preview = app.ui.prefs.thumbnails && super::node_style::shows_preview(&outs) && !app.graph.thumbnails_cleared;
        let result = app.session.cooked.nodes.get(&node.id);
        let image = result.and_then(|r| r.outputs.first()).and_then(materialcraft_engine::eval::NodeOutput::as_image);
        let body_height = if preview { CARD_BODY_HEIGHT } else { BARE_BODY_HEIGHT }.max(ins.len().max(outs.len()) as f32 * PORT_GAP + PORT_PADDING);
        let body_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(NODE_W, body_height));
        let thumbnail_side = body_rect.width().min(body_rect.height());
        let texture = image
            .filter(|_| preview)
            .map(|image| {
                app.thumbs.nodes(ctx).state(
                    ctx,
                    &NodeFace { image },
                    &NodeThumb { cook: result.map_or(0, |r| r.key), port: 0 },
                    &ThumbRequest::for_cell(thumbnail_side, ctx.pixels_per_point()),
                    Priority::VISIBLE,
                )
            })
            .and_then(|state| state.shown())
            .map(|texture| (texture.id, texture.uv()));
        let graph = app.session.graph().ok();
        let bits = result.map_or_else(|| graph.as_ref().map_or("16", |g| g.format.bits()), |r| r.base.format.bits());
        let footer = result.and_then(|r| {
            image.map(|image| {
                if app.session.preview_size.is_some() {
                    format!("{}x{} {}", image.width(), image.height(), crate::i18n::tr("Preview"))
                } else {
                    format!("{}x{} - {}{bits}", image.width(), image.height(), if image.is_gray() { "L" } else { "C" })
                }
            })
        });
        let value = result.and_then(|r| r.outputs.first()).and_then(materialcraft_engine::eval::NodeOutput::as_value).map(super::value_text::value_text);
        let status = result
            .map(|r| if r.error.is_some() { "warning".to_string() } else { "".to_string() })
            .filter(|s| !s.is_empty())
            .or_else(|| result.is_none().then(|| "hourglass".to_string()));
        let title = super::node_style::node_label(node);
        let tooltip = tooltip(app, node.id);
        Self { title, def: node.def.clone(), ins, outs, body_height, footer, tooltip, texture, value, status, dot: node.def == "dot", disabled: node.disabled }
    }

    /// The node card's document rectangle, with its top-left corner at `pos`.
    pub(crate) fn rect(&self, pos: [f32; 2]) -> egui::Rect {
        if self.dot {
            return Self::bounds(pos, self.ins.len(), self.outs.len(), false, true);
        }
        egui::Rect::from_min_size(egui::pos2(pos[0], pos[1]), egui::vec2(NODE_W, HEADER_HEIGHT + self.body_height + FOOTER_HEIGHT))
    }

    fn bounds(position: [f32; 2], inputs: usize, outputs: usize, preview: bool, dot: bool) -> egui::Rect {
        let position = egui::pos2(position[0], position[1]);
        if dot {
            return egui::Rect::from_center_size(position, egui::vec2(DOT_DIAMETER, DOT_DIAMETER));
        }
        let body = if preview { CARD_BODY_HEIGHT } else { BARE_BODY_HEIGHT };
        let body = body.max(inputs.max(outputs) as f32 * PORT_GAP + PORT_PADDING);
        egui::Rect::from_min_size(position, egui::vec2(NODE_W, HEADER_HEIGHT + body + FOOTER_HEIGHT))
    }
}

/// The localized description of a node definition or its limited preview.
pub(crate) fn tooltip(app: &MaterialCraftApp, node: materialcraft_engine::core::NodeId) -> Option<String> {
    let graph = app.session.graph().ok()?;
    let node = graph.node(node).ok()?;
    if let Some(error) = app.session.cooked.nodes.get(&node.id).and_then(|result| result.error.as_ref()) {
        return Some(error.clone());
    }
    let result = app.session.cooked.nodes.get(&node.id);
    let image = result.and_then(|result| result.outputs.first()).and_then(materialcraft_engine::eval::NodeOutput::as_image);
    if let (Some(image), Some(limit), Some(result)) = (image, app.session.preview_size, result)
        && let Some(graph) = app.session.graph().ok()
        && let [width, height] = graph.size_px()
    {
        let bits = result.base.format.bits();
        return Some(format!(
            "{} x {} ({bits}bpc)\n{}: {width} x {height}\n{}: {limit} px\n{}",
            image.width(),
            image.height(),
            crate::i18n::tr("Parent size"),
            crate::i18n::tr("Preview limit"),
            crate::i18n::tr("Change Preview size while editing in Edit > Preferences. Exports use each node's configured output size.")
        ));
    }
    materialcraft_engine::nodes::find(&node.def).map(|definition| crate::i18n::tr(definition.description).to_string()).filter(|text| !text.is_empty())
}
