//! What a function node shows in its body: its editable value, or the symbol of its operation

use craft_ui::contract::Gap;
use craft_ui::style::StyleCx;
use egui::Ui;
use egui_phosphor::regular as icon;
use materialcraft_engine::core::FunctionNode;

pub(super) const PADDING: f32 = 6.0;

/// The graph-space height of one value field and the gap between two, as the face's field group lays them out.
#[derive(Clone, Copy)]
pub(super) struct Rows {
    height: f32,
    gap: f32,
}

impl Rows {
    /// The theme's control height and a field group's row gap, in the tokens `ui` shows with.
    pub(super) fn of(ui: &Ui) -> Self {
        Rows { height: StyleCx::with_root(ui.ctx(), |cx| cx.control().height), gap: Gap::Row.points(ui) }
    }
}
/// Body height of a node that shows a symbol
pub(super) const SYMBOL_HEIGHT: f32 = 40.0;
/// Below this zoom the fields are too small to use, so the value is drawn as text
pub(super) const MIN_EDIT_ZOOM: f32 = 0.45;

/// The sign or short word that stands for `operation` on its node
pub(super) fn symbol(operation: &str) -> &'static str {
    match operation {
        "add" => "+",
        "sub" => "\u{2212}",
        "mul" => "\u{d7}",
        "mulscalar" => "k\u{d7}",
        "div" => "\u{f7}",
        "neg" => "\u{2212}a",
        "mod" => icon::PERCENT,
        "dot" => "a\u{b7}b",
        "and" => "AND",
        "or" => "OR",
        "not" => "NOT",
        "eq" => icon::EQUALS,
        "noteq" => icon::NOT_EQUALS,
        "gt" => icon::GREATER_THAN,
        "gteq" => icon::GREATER_THAN_OR_EQUAL,
        "lr" => icon::LESS_THAN,
        "lreq" => icon::LESS_THAN_OR_EQUAL,
        "abs" => "|x|",
        "floor" => "floor",
        "ceil" => "ceil",
        "cos" => "cos",
        "sin" => "sin",
        "tan" => "tan",
        "atan2" => "atan2",
        "cartesian" => "polar",
        "sqrt" => icon::RADICAL,
        "log" => "log",
        "exp" => "exp",
        "log2" => "log2",
        "pow2" => "2^x",
        "pow" => "a^b",
        "lerp" => "lerp",
        "min" => "min",
        "max" => "max",
        "rand" => icon::DICE_FIVE,
        "sequence" => icon::LIST_NUMBERS,
        "ifelse" => "if",
        "while" => icon::REPEAT,
        "vector2" | "ivector2" => "xy",
        "vector3" | "ivector3" => "xyz",
        "vector4" | "ivector4" => "xyzw",
        "tofloat" => "float",
        "tofloat2" => "float2",
        "tofloat3" => "float3",
        "tofloat4" => "float4",
        "toint1" => "int",
        "toint2" => "int2",
        "toint3" => "int3",
        "toint4" => "int4",
        "samplelum" => icon::EYEDROPPER,
        "samplecol" => icon::EYEDROPPER_SAMPLE,
        _ => icon::FUNCTION,
    }
}

/// Graph-space height the body of `node` needs for its value fields or its symbol
pub(super) fn height(node: &FunctionNode, metrics: Rows) -> f32 {
    match &node.value {
        Some(value) if node.missing.is_none() => {
            let rows = crate::value_editor::rows(value) as f32;
            rows * metrics.height + (rows - 1.0) * metrics.gap + PADDING * 2.0
        }
        _ => SYMBOL_HEIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_engine::nodes::function::{FUNCTIONS, default_value};

    #[test]
    fn every_operation_without_a_value_has_its_own_symbol() {
        for definition in FUNCTIONS {
            if definition.id == "passthrough" || default_value(definition.id).is_some() {
                continue;
            }

            assert_ne!(symbol(definition.id), icon::FUNCTION, "{} has no symbol", definition.id);
        }
    }
}
