//! Data colours used by the material graph.

use craft_theme::{Rgba, Tokens};
use craft_ui::style::TokenGroup;
use materialcraft_engine::{core::PatternNodeKind, nodes::PortKind};

/// Data colours used by material, Function and FX-Map nodes.
#[derive(Clone, Debug, TokenGroup)]
#[group(path = "materialcraft.node")]
pub(crate) struct NodeColors {
    header_output: Rgba,
    header_input: Rgba,
    header_instance: Rgba,
    header_atomic: Rgba,
    header_noise: Rgba,
    header_pattern: Rgba,
    header_material: Rgba,
    header_filter: Rgba,
    port_gray: Rgba,
    port_color: Rgba,
    port_value: Rgba,
    port_any: Rgba,
    function_constant: Rgba,
    function_variables: Rgba,
    function_samplers: Rgba,
    function_vector: Rgba,
    function_operator: Rgba,
    function_logical: Rgba,
    function_fallback: Rgba,
    function_output_color: Rgba,
    function_missing_color: Rgba,
    fx_quadrant: Rgba,
    fx_switch: Rgba,
    fx_iterate: Rgba,
    fx_dot: Rgba,
    fx_symbol_color: Rgba,
    fx_port_color: Rgba,
    fx_root_color: Rgba,
}

impl NodeColors {
    fn derive(_: &Tokens) -> Self {
        Self {
            header_output: Rgba::rgb(0x2c, 0x8f, 0x5e),
            header_input: Rgba::rgb(0x6d, 0x52, 0xb5),
            header_instance: Rgba::rgb(0xa2, 0x3f, 0x4a),
            header_atomic: Rgba::rgb(0x5c, 0x62, 0x6e),
            header_noise: Rgba::rgb(0xa8, 0x3c, 0x34),
            header_pattern: Rgba::rgb(0xb3, 0x5a, 0x2c),
            header_material: Rgba::rgb(0x3a, 0x66, 0xb0),
            header_filter: Rgba::rgb(0x3b, 0x7f, 0x4c),
            port_gray: Rgba::rgb(0xc8, 0xc8, 0xc8),
            port_color: Rgba::rgb(0xe8, 0xa9, 0x3c),
            port_value: Rgba::rgb(0x8b, 0xc6, 0x74),
            port_any: Rgba::rgb(0x8c, 0xb4, 0xe0),
            function_constant: Rgba::rgb(0x5c, 0x62, 0x6e),
            function_variables: Rgba::rgb(0x6d, 0x52, 0xb5),
            function_samplers: Rgba::rgb(0xa8, 0x3c, 0x34),
            function_vector: Rgba::rgb(0x3a, 0x66, 0xb0),
            function_operator: Rgba::rgb(0x3b, 0x7f, 0x4c),
            function_logical: Rgba::rgb(0xb3, 0x5a, 0x2c),
            function_fallback: Rgba::rgb(0x1e, 0x8a, 0xa0),
            function_output_color: Rgba::rgb(255, 163, 26),
            function_missing_color: Rgba::rgb(210, 35, 125),
            fx_quadrant: Rgba::rgb(230, 157, 31),
            fx_switch: Rgba::rgb(75, 153, 109),
            fx_iterate: Rgba::rgb(79, 131, 185),
            fx_dot: Rgba::rgb(180, 180, 180),
            fx_symbol_color: Rgba::rgb(35, 35, 35),
            fx_port_color: Rgba::rgb(165, 157, 215),
            fx_root_color: Rgba::rgb(255, 215, 0),
        }
    }

    /// The header colour for a material node definition.
    pub(crate) fn header(&self, def: &str) -> Rgba {
        let category = materialcraft_engine::nodes::find(def).map_or("", |d| d.category);
        match def {
            "output" => self.header_output,
            "input_color" | "input_grayscale" | "input_value" => self.header_input,
            "graph_instance" => self.header_instance,
            _ if category.starts_with("Atomic") => self.header_atomic,
            _ if category.contains("Noises") => self.header_noise,
            _ if category.contains("Patterns") => self.header_pattern,
            _ if category.starts_with("Material") => self.header_material,
            _ if category.starts_with("Filters") => self.header_filter,
            _ => self.header_atomic,
        }
    }

    /// The colour of a material port.
    pub(crate) fn port(&self, kind: PortKind) -> Rgba {
        match kind {
            PortKind::Gray => self.port_gray,
            PortKind::Color => self.port_color,
            PortKind::Value => self.port_value,
            PortKind::Any => self.port_any,
        }
    }

    /// The header colour for a Function category.
    pub(crate) fn function_category(&self, category: &str) -> Rgba {
        match category {
            "Constant" => self.function_constant,
            "Variables" => self.function_variables,
            "Samplers" => self.function_samplers,
            "Vector" | "Cast" => self.function_vector,
            "Operator" => self.function_operator,
            "Logical" | "Comparison" | "Control" => self.function_logical,
            _ => self.function_fallback,
        }
    }

    /// The colour of a Function output node.
    pub(crate) fn function_output(&self) -> Rgba {
        self.function_output_color
    }
    /// The colour of a missing Function operation.
    pub(crate) fn function_missing(&self) -> Rgba {
        self.function_missing_color
    }
    /// The colour of an FX-Map node kind.
    pub(crate) fn fx_kind(&self, kind: PatternNodeKind) -> Rgba {
        match kind {
            PatternNodeKind::Quadrant => self.fx_quadrant,
            PatternNodeKind::Switch => self.fx_switch,
            PatternNodeKind::Iterate => self.fx_iterate,
            PatternNodeKind::Dot => self.fx_dot,
        }
    }
    /// The colour of an FX-Map symbol.
    pub(crate) fn fx_symbol(&self) -> Rgba {
        self.fx_symbol_color
    }
    /// The colour of an FX-Map root.
    pub(crate) fn fx_root(&self) -> Rgba {
        self.fx_root_color
    }
    /// The colour of an FX-Map port.
    pub(crate) fn fx_port(&self) -> Rgba {
        self.fx_port_color
    }
}
