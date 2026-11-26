use crate::i18n::tr;
use egui::Ui;
use materialcraft_engine::{
    FunctionGraphTarget,
    core::{FunctionGraph, Graph},
};

pub(super) fn show(ui: &mut Ui, document: &Graph, target: &FunctionGraphTarget, graph: &FunctionGraph) {
    let messages = materialcraft_engine::function_inspection::warnings_for(document, target, graph);
    let expected = materialcraft_engine::function_output::expected_type(document, target);
    if messages.is_empty() && expected.is_none() {
        return;
    }
    let _ = craft_ui::layout_components::Column::new().show(ui, |ui, _| {
        if let Some(expected) = expected {
            let _ = craft_ui::display::Text::new(format!("{}: {}", tr("Expected output"), expected.label())).shade(craft_ui::display::Shade::Muted).show(ui);
        }
        for message in messages {
            craft_ui::notices::Notice::new(craft_theme::variant::Tone::Warn, message)
                .show_with_tip(ui, |ui| {
                    let _ = craft_ui::display::Text::new(tr("Choose an output of the expected type and connect it to a value source, such as a constant or variable input"))
                        .show(ui);
                });
        }
    });
}
