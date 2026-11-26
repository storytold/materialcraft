//! The value field editor every literal row uses: number fields per component, a check box or a text field. It
//! returns the controls' edit events, each carrying the whole value with its edited component replaced, so a caller
//! sends them through `edit_commit::send` and the undo step closes on commit.

use craft_behavior::value::EditEvents;
use craft_ui::choice::Checkbox;
use craft_ui::contract::{Bind, FieldLayout, GlyphLabel, LabelKind, PaintUi, Props as _, Show as _, ShowContainer as _, SideLabel, Sizing};
use craft_ui::form::{Field, FieldGroup};
use craft_ui::layout_components::Row;
use craft_ui::text::{NumberField, TextField};
use egui::{Rect, Ui};
use materialcraft_engine::core::Value;

use crate::i18n::tr;

/// The caption of a value with one field.
static VALUE: [&str; 1] = ["Value"];
/// The captions of a vector's components, in index order.
static AXES: [&str; 4] = ["X", "Y", "Z", "W"];
/// The captions of a colour's channels, in index order.
static CHANNELS: [&str; 4] = ["Red", "Green", "Blue", "Alpha"];

/// Edit a function value in one row of captioned fields, wrapped to the available width
pub(crate) fn show(ui: &mut Ui, value: &Value) -> EditEvents<Value> {
    Row::new().fill_width().wrap(true).show(ui, |ui, _| fields(ui, value, false)).inner.unwrap_or_default()
}

/// Edit a function value with one full-width field per component at `rect`, as a node face shows it
pub(crate) fn show_stacked(ui: &mut PaintUi<'_>, rect: Rect, value: &Value) -> EditEvents<Value> {
    ui.container("value", rect, FieldGroup::new(tr("Value")), |ui, _| fields(ui, value, true)).inner.unwrap_or_default()
}

/// How many fields `show_stacked` lays out for `value`
pub(crate) fn rows(value: &Value) -> usize {
    match value {
        Value::Float2(_) | Value::Int2(_) => 2,
        Value::Float3(_) | Value::Int3(_) => 3,
        Value::Float4(_) | Value::Int4(_) | Value::Color(_) => 4,
        _ => 1,
    }
}

/// The numeric components of a value, in index order.
enum Parts<'a> {
    Floats(&'a mut [f32]),
    Ints(&'a mut [i32]),
}

/// The components of `value` and their captions; `None` for a value that is not a number or a vector of them.
fn parts(value: &mut Value) -> Option<(Parts<'_>, &'static [&'static str])> {
    Some(match value {
        Value::Float(v) => (Parts::Floats(std::slice::from_mut(v)), &VALUE[..]),
        Value::Float2(v) => (Parts::Floats(&mut v[..]), &AXES[..2]),
        Value::Float3(v) => (Parts::Floats(&mut v[..]), &AXES[..3]),
        Value::Float4(v) => (Parts::Floats(&mut v[..]), &AXES[..]),
        Value::Color(v) => (Parts::Floats(&mut v[..]), &CHANNELS[..]),
        Value::Int(v) => (Parts::Ints(std::slice::from_mut(v)), &VALUE[..]),
        Value::Int2(v) => (Parts::Ints(&mut v[..]), &AXES[..2]),
        Value::Int3(v) => (Parts::Ints(&mut v[..]), &AXES[..3]),
        Value::Int4(v) => (Parts::Ints(&mut v[..]), &AXES[..]),
        _ => return None,
    })
}

/// `value` with component `index` set to `number`, rounded for an integer component.
fn with_component(value: &Value, index: usize, number: f64) -> Value {
    let mut value = value.clone();
    match parts(&mut value) {
        Some((Parts::Floats(components), _)) => {
            if let Some(component) = components.get_mut(index) {
                *component = number as f32;
            }
        }
        Some((Parts::Ints(components), _)) => {
            if let Some(component) = components.get_mut(index) {
                *component = number.round() as i32;
            }
        }
        None => {}
    }
    value
}

fn fields(ui: &mut Ui, value: &Value, stacked: bool) -> EditEvents<Value> {
    match value {
        Value::Bool(on) => {
            let shown = if !stacked {
                LabelKind::Hidden
            } else if *on {
                LabelKind::Short(tr("True").into())
            } else {
                LabelKind::Short(tr("False").into())
            };
            let side = SideLabel::new().layout(FieldLayout::After).kind(shown);
            Field::new(tr("Value"))
                .side(side)
                .show(ui, |ui, _| Checkbox::new(Bind::given(*on)).label(tr("Value")).show(ui).events.map(Value::Bool))
                .inner
                .unwrap_or_default()
        }
        Value::Text(text) => TextField::new(tr("Value"), Bind::given(text.clone())).sizing(Sizing::FillWidth).show(ui).events.map(Value::Text),
        _ => numbers(ui, value, stacked),
    }
}

/// One number field per component, each edit carrying the whole value.
fn numbers(ui: &mut Ui, value: &Value, stacked: bool) -> EditEvents<Value> {
    let mut shown = value.clone();
    let Some((components, captions)) = parts(&mut shown) else { return EditEvents::new() };
    let (numbers, integer): (Vec<f64>, bool) = match components {
        Parts::Floats(components) => (components.iter().map(|&c| f64::from(c)).collect(), false),
        Parts::Ints(components) => (components.iter().map(|&c| f64::from(c)).collect(), true),
    };
    let mut events = EditEvents::new();
    for (index, (number, caption)) in numbers.into_iter().zip(captions).enumerate() {
        let caption = tr(*caption);
        let edits = if stacked {
            // The component's first letter sits inside the field's box; the single value field shows no caption.
            let kind = if captions.len() == 1 {
                LabelKind::Hidden
            } else {
                LabelKind::Glyph(GlyphLabel::caption(caption.chars().next().map(String::from).unwrap_or_default()))
            };
            let side = SideLabel::new().layout(FieldLayout::Inside).kind(kind);
            Field::new(caption).side(side).show(ui, |ui, _| number_field(ui, caption, number, integer)).inner.unwrap_or_default()
        } else {
            number_field(ui, caption, number, integer)
        };
        events.extend(edits.map(|typed| with_component(value, index, typed.unwrap_or(number))));
    }
    events
}

/// A number field named `caption`: a float shows up to 3 decimals and moves by hundredths; an integer shows whole
/// numbers, steps by 1 and scrubs a tenth per point.
fn number_field(ui: &mut Ui, caption: &'static str, number: f64, integer: bool) -> EditEvents<Option<f64>> {
    let field = NumberField::new(caption, Bind::given(Some(number)));
    let field = if integer { field.scrub_step(0.1).fixed_decimals(0).commit_decimals(0) } else { field.step(0.01).max_decimals(3) };
    field.show(ui).edit.events
}
