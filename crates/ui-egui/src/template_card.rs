//! The template cards of the New graph dialog and the home screen: a name, the kind of graph, and a description of
//! what the template holds

use materialcraft_engine::core::GraphKind;

use crate::i18n::tr;

/// A template a new graph can start from
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemplateCard {
    /// The `file.new` template id; function graphs have one template
    pub id: &'static str,
    pub label: &'static str,
    pub kind: GraphKind,
    pub description: &'static str,
}

/// The id the function graph card goes by
pub(crate) const FUNCTION_TEMPLATE: &str = "function";

/// Every card: the material templates the engine has, then the function graph
pub(crate) fn cards() -> Vec<TemplateCard> {
    let material =
        materialcraft_engine::samples::TEMPLATES.iter().map(|(id, label)| TemplateCard { id, label, kind: GraphKind::Material, description: description(id) });
    let function = TemplateCard { id: FUNCTION_TEMPLATE, label: "Function graph", kind: GraphKind::Function, description: description(FUNCTION_TEMPLATE) };
    material.chain(std::iter::once(function)).collect()
}

/// What a template holds, in a sentence
fn description(id: &str) -> &'static str {
    match id {
        "empty" => "An empty material graph",
        "pbr" => "Base color, normal, roughness, metallic, height and ambient occlusion outputs with inputs already connected",
        "atomics" => "A small graph of atomic nodes working together",
        FUNCTION_TEMPLATE => "An empty function graph: a value computed from function nodes",
        _ => "",
    }
}

impl TemplateCard {
    /// The name the kind of graph goes by under the card's title
    pub fn kind_label(&self) -> &'static str {
        if self.kind == GraphKind::Function { "Function" } else { "Material" }
    }
}

/// The start screen's card of a template: its picture is keyed by the id, its description is the info mark's tip
pub(crate) fn template(card: &TemplateCard) -> craft_ui::shell::Template {
    craft_ui::shell::Template::new(card.id, tr(card.label), tr(card.kind_label())).description(tr(card.description))
}
