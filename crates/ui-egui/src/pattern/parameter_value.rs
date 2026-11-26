use crate::{MaterialCraftApp, i18n::tr};
use craft_ui::contract::Show as _;
use egui::Ui;
use materialcraft_engine::core::Value;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, owner: materialcraft_engine::core::NodeId, node: u32, parameter: &str, value: &Value) {
    let options: &[(i32, &str)] = match parameter {
        "patterntype" => &[
            (0, "None"),
            (1, "Input image"),
            (2, "Square"),
            (3, "Disc"),
            (4, "Paraboloid"),
            (5, "Bell"),
            (6, "Gaussian"),
            (7, "Thorn"),
            (8, "Pyramid"),
            (9, "Brick"),
            (10, "Gradation"),
            (11, "Waves"),
            (12, "Half bell"),
            (13, "Ridged bell"),
            (14, "Crescent"),
            (15, "Capsule"),
            (16, "Cone"),
            (17, "Hemisphere"),
            (18, "Offset gradation"),
        ],
        "blendingmode" => &[(0, "Add"), (1, "Blend"), (2, "Max")],
        "imagefiltering" => &[(0, "Nearest"), (1, "Bilinear")],
        _ => {
            let events = crate::value_editor::show(ui, value);
            let _ = crate::edit_commit::send(
                app,
                &events,
                "graph.pattern.edit",
                |v| serde_json::json!({"id":owner.0,"edit":materialcraft_engine::PatternEdit::Value{id:node,parameter:parameter.into(),value:Some(v.clone())},"merge":true}),
            );
            return;
        }
    };
    if let Some(current) = value.as_i32() {
        let entries: Vec<_> = options.iter().map(|(id, label)| craft_ui::contract::Entry::new(*id, tr(label))).collect();
        let mut selected = current;
        let response = craft_ui::pickers::Select::new(tr(parameter), craft_ui::contract::Bind::bound(&mut selected), &entries)
            .placeholder(tr("Custom"))
            .show(ui);
        let events = response.edit.events.map(Value::Int);
        let _ = crate::edit_commit::send(
            app,
            &events,
            "graph.pattern.edit",
            |v| serde_json::json!({"id":owner.0,"edit":materialcraft_engine::PatternEdit::Value{id:node,parameter:parameter.into(),value:Some(v.clone())},"merge":true}),
        );
    }
}
