//! The Function graph's node kind: the card the framework node graph draws for a Function node.

use craft_behavior::EditEvents;
use craft_behavior::value::EditEvent;
use craft_ui::choice::Checkbox;
use craft_ui::contract::{Bind, MeasureCx, PaintUi, Props as _};
use craft_ui::graph::{Node, NodeHeader, NodeKind, NodeShape, PortDirection};
use craft_ui::primitives::{Draw, HAlign, PaintShape, TextRun};
use craft_ui::style::{DataColor, StyleCx, TypeRole, rgba_color};
use craft_ui::text::{NumberField, TextField};
use egui::{CornerRadius, Rect, Vec2, pos2, vec2};
use materialcraft_engine::core::{FunctionNode, Value};
use materialcraft_engine::nodes::function as functions;

use super::node_face::{self, MIN_EDIT_ZOOM, PADDING, ROW_GAP, ROW_HEIGHT, SYMBOL_HEIGHT};
use super::port_label::port_label;
use crate::graph::node_colors::NodeColors;
use crate::graph::{FOOTER_HEIGHT, HEADER_HEIGHT, NODE_W};

/// The spacing of the card's inputs, in graph units.
const PORT_GAP: f32 = 18.0;
/// The side of a Dot, centred on its position.
const DOT_SIDE: f32 = 14.0;

/// A Function node as the framework node graph draws it, with the inline value control above the edit zoom.
pub(crate) struct FunctionKind<'a> {
    node: &'a FunctionNode,
    inputs: Vec<&'a str>,
    /// The node is the graph's output node: its body takes the output colour.
    output: bool,
    /// The canvas zoom in points per graph unit; the body paint has no env to read it from.
    scale: f32,
}

/// The edit event of a Function node's inline value control.
pub(crate) struct FunctionValueEdit {
    /// The Function node.
    pub id: u32,
    /// What the control reported, as the node's whole new value.
    pub edit: EditEvent<Value>,
}

impl<'a> FunctionKind<'a> {
    /// The kind of `node`; `output` when it is its graph's output node, `scale` the canvas zoom of this pass.
    pub(crate) fn new(node: &'a FunctionNode, output: bool, scale: f32) -> Self {
        FunctionKind { node, inputs: functions::input_identifiers(node).collect(), output, scale }
    }

    /// The card of `node` at `pos`, in graph units: the body fits the value rows or the symbol and the inputs, a Dot is
    /// a small circle centred on `pos`. The same at every zoom, so the canvas frames it before the pass.
    pub(crate) fn rect(node: &FunctionNode, pos: [f32; 2]) -> Rect {
        let pos = pos2(pos[0], pos[1]);
        if is_dot(node) {
            return Rect::from_center_size(pos, Vec2::splat(DOT_SIDE));
        }
        let ports = functions::input_identifiers(node).count().max(1) as f32 * PORT_GAP + 8.0;
        Rect::from_min_size(pos, vec2(NODE_W, HEADER_HEIGHT + node_face::height(node).max(ports) + FOOTER_HEIGHT))
    }

    fn dot(&self) -> bool {
        is_dot(self.node)
    }

    /// The index of `node`'s output port: after every input.
    pub(crate) fn output_port_of(node: &FunctionNode) -> usize {
        functions::input_identifiers(node).count()
    }

    fn output_port(&self) -> usize {
        self.inputs.len()
    }

    /// An unknown operation, or an instance whose source graph did not load.
    fn missing(&self) -> bool {
        self.node.missing.is_some() || (self.node.instance.is_some() && !matches!(self.node.source, Some(Ok(_))))
    }

    /// The value fields show at `scale` and up; below it the value is drawn as text, as the fields are too small to use.
    fn fields_at(&self, scale: f32) -> Option<&'a Value> {
        self.node.value.as_ref().filter(|_| !self.dot() && !self.missing() && scale >= MIN_EDIT_ZOOM)
    }

    /// The body between the header and the footer of a card at `rect`.
    fn body(rect: Rect) -> Rect {
        Rect::from_min_max(pos2(rect.left(), rect.top() + HEADER_HEIGHT), pos2(rect.right(), rect.bottom() - FOOTER_HEIGHT))
    }
}

/// A passthrough node is drawn as a Dot.
fn is_dot(node: &FunctionNode) -> bool {
    node.op == "passthrough"
}

/// The components of `value` as the stacked fields show them: each field's name and number.
fn components(value: &Value) -> Vec<(&'static str, f64)> {
    fn named(names: &[&'static str], values: &[f32]) -> Vec<(&'static str, f64)> {
        names.iter().zip(values).map(|(name, v)| (*name, f64::from(*v))).collect()
    }
    fn numbered(values: &[i32]) -> Vec<(&'static str, f64)> {
        ["X", "Y", "Z", "W"].iter().zip(values).map(|(name, v)| (*name, f64::from(*v))).collect()
    }
    match value {
        Value::Float(v) => vec![("Value", f64::from(*v))],
        Value::Float2(v) => named(&["X", "Y"], v),
        Value::Float3(v) => named(&["X", "Y", "Z"], v),
        Value::Float4(v) => named(&["X", "Y", "Z", "W"], v),
        Value::Color(v) => named(&["Red", "Green", "Blue", "Alpha"], v),
        Value::Int(v) => vec![("Value", f64::from(*v))],
        Value::Int2(v) => numbered(v),
        Value::Int3(v) => numbered(v),
        Value::Int4(v) => numbered(v),
        _ => Vec::new(),
    }
}

/// `value` with component `at` set to `number`; integers round.
fn with_component(value: &Value, at: usize, number: f64) -> Value {
    let mut value = value.clone();
    let float = number as f32;
    let int = number.round() as i32;
    match &mut value {
        Value::Float(v) => *v = float,
        Value::Int(v) => *v = int,
        Value::Float2(v) => v.get_mut(at).into_iter().for_each(|c| *c = float),
        Value::Float3(v) => v.get_mut(at).into_iter().for_each(|c| *c = float),
        Value::Float4(v) | Value::Color(v) => v.get_mut(at).into_iter().for_each(|c| *c = float),
        Value::Int2(v) => v.get_mut(at).into_iter().for_each(|c| *c = int),
        Value::Int3(v) => v.get_mut(at).into_iter().for_each(|c| *c = int),
        Value::Int4(v) => v.get_mut(at).into_iter().for_each(|c| *c = int),
        _ => {}
    }
    value
}

/// The last event of a pass, which carries the latest value of the running gesture.
fn last<T, U>(events: EditEvents<T>, f: impl Fn(T) -> Option<U>) -> Option<EditEvent<U>> {
    events.into_iter().filter_map(|event| event.filter_map(&f)).last()
}

impl NodeKind<FunctionValueEdit> for FunctionKind<'_> {
    fn title(&self) -> &str {
        functions::node_label(self.node)
    }

    fn port_count(&self) -> usize {
        self.inputs.len() + 1
    }

    fn port_name(&self, port: usize) -> String {
        port_label(self.inputs.get(port).copied().unwrap_or("output")).to_owned()
    }

    /// Inputs down the left edge, centred on the body; the output on the right; a Dot's two ports at its middle.
    fn port_offset(&self, port: usize, node_size: Vec2) -> Vec2 {
        let output = port == self.output_port();
        let x = if output { node_size.x } else { 0.0 };
        if self.dot() {
            return vec2(x, node_size.y / 2.0);
        }
        let (index, count) = if output { (0, 1) } else { (port, self.inputs.len()) };
        let body = node_size.y - HEADER_HEIGHT - FOOTER_HEIGHT;
        let first = HEADER_HEIGHT + (body - count as f32 * PORT_GAP) * 0.5 + PORT_GAP * 0.5;
        vec2(x, first + index as f32 * PORT_GAP)
    }

    fn port_direction(&self, port: usize) -> PortDirection {
        if port == self.output_port() { PortDirection::Output } else { PortDirection::Input }
    }

    fn measure_controls(&self, cx: &MeasureCx<'_, Node<'_, FunctionValueEdit>>) -> Vec2 {
        match self.fields_at(cx.env.view_scale()) {
            Some(_) => vec2(0.0, node_face::height(self.node)),
            None if self.dot() => Vec2::ZERO,
            None => vec2(0.0, SYMBOL_HEIGHT),
        }
    }

    // PLAN: sh.A20's `value_editor::show_stacked` takes an egui `Ui`, which `PaintUi` does not lend; the node shows the
    // same stacked fields (one per component, as `value_editor::rows`) as framework children here.
    fn controls(&self, ui: &mut PaintUi<'_>, rect: Rect) -> Option<FunctionValueEdit> {
        let value = self.fields_at(self.scale)?;
        let content = rect.shrink(PADDING);
        let row = |at: usize| Rect::from_min_size(pos2(content.left(), content.top() + at as f32 * (ROW_HEIGHT + ROW_GAP)), vec2(content.width(), ROW_HEIGHT));
        let edit = match value {
            Value::Bool(checked) => last(ui.child("value", row(0), Checkbox::new(Bind::given(*checked)).label("Value")).events, |b| Some(Value::Bool(b))),
            Value::Text(text) => last(ui.child("value", row(0), TextField::new("Value", Bind::given(text.clone()))).events, |t| Some(Value::Text(t))),
            value => components(value).into_iter().enumerate().fold(None, |edit, (at, (name, number))| {
                let events = ui.child(("value", at), row(at), NumberField::new(name, Bind::given(Some(number)))).edit.events;
                last(events, |n: Option<f64>| n.map(|n| with_component(value, at, n))).or(edit)
            }),
        }?;
        Some(FunctionValueEdit { id: self.node.id, edit })
    }

    fn paint_body(&self, draw: &Draw<'_>, rect: Rect, cx: &StyleCx<'_>) {
        if self.dot() {
            return;
        }
        let colors = cx.token_group::<NodeColors>();
        let body = Self::body(rect);
        let mut ink = None;
        if self.output {
            let tone = DataColor::new(rgba_color(colors.function_output()));
            draw.data_fill(&PaintShape::Rect(body, CornerRadius::ZERO), tone);
            ink = Some(cx.ink_on(tone));
        }
        let shown: Option<String> = if self.missing() {
            Some(crate::i18n::tr("MISSING").to_owned())
        } else if let Some(value) = &self.node.value {
            self.fields_at(self.scale).is_none().then(|| crate::graph::value_text::value_text(value))
        } else if self.node.instance.is_some() {
            Some("ƒ".to_owned())
        } else {
            Some(node_face::symbol(&self.node.op).to_owned())
        };
        if let Some(shown) = shown {
            let run = TextRun::new(shown).role(TypeRole::Heading).align(HAlign::Center).elide();
            let run = match ink {
                Some(ink) => run.color(ink),
                None => run,
            };
            draw.text(body.shrink(PADDING), &run);
        }
    }

    fn header(&self, cx: &StyleCx<'_>) -> Option<NodeHeader> {
        if self.dot() {
            return None;
        }
        let colors = cx.token_group::<NodeColors>();
        let tone = if self.missing() {
            colors.function_missing()
        } else if self.node.instance.is_some() {
            colors.header("graph_instance")
        } else {
            colors.function_category(functions::definition(&self.node.op).map_or("", |definition| definition.category))
        };
        Some(NodeHeader::new(DataColor::new(rgba_color(tone)), HEADER_HEIGHT))
    }

    fn port_tone(&self, _port: usize, cx: &StyleCx<'_>) -> Option<DataColor> {
        Some(DataColor::new(rgba_color(cx.token_group::<NodeColors>().port(materialcraft_engine::nodes::PortKind::Value))))
    }

    fn shape(&self) -> NodeShape {
        if self.dot() { NodeShape::Dot } else { NodeShape::Card }
    }
}
