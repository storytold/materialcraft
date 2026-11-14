use super::{graph_property_draft::GraphPropertyDraft, section};
use crate::{MaterialCraftApp, i18n::tr};
use materialcraft_engine::GraphTarget;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui, target: GraphTarget) {
    if app.graph_property_draft.as_ref().is_some_and(|draft| draft.target != target) {
        GraphPropertyDraft::commit(app);
    }

    let Ok(graph) = target.get(&app.session) else { return };
    let draft = &mut app.graph_property_draft;
    section(ui, "graph-attributes", tr("Attributes"), |ui| {
        for (key, label, current) in [
            ("name", "Identifier", &graph.name),
            ("description", "Description", &graph.attributes.description),
            ("category", "Category", &graph.attributes.category),
            ("label", "Label", &graph.attributes.label),
            ("author", "Author", &graph.attributes.author),
            ("authorUrl", "Author URL", &graph.attributes.author_url),
            ("tags", "Tags", &graph.attributes.tags),
            ("group", "Group", &graph.attributes.group),
            ("userData", "User Data", &graph.attributes.user_data),
        ] {
            if graph.function.is_none() && !matches!(key, "name" | "label") {
                continue;
            }

            let widget = ui.make_persistent_id((target, key));
            if draft.as_ref().is_some_and(|draft| draft.widget == widget && draft.original != *current) {
                *draft = None;
            }

            let pending = draft.as_ref().filter(|draft| draft.widget == widget && draft.original == *current);
            let mut text = pending.map_or_else(|| current.clone(), |draft| draft.text.clone());
            attribute_label(ui, tr(label), crate::i18n::is_rtl());
            let editor = if key == "description" { egui::TextEdit::multiline(&mut text).desired_rows(6) } else { egui::TextEdit::singleline(&mut text) };
            let response = ui.add(editor.id(widget).desired_width(ui.available_width()));
            crate::widgets::name(&response, egui::Role::TextInput, label);
            if response.changed() {
                *draft = Some(GraphPropertyDraft { target, key, original: current.clone(), text, widget });
            }
            ui.add_space(5.0);
        }
    });
    if app.graph_property_draft.as_ref().is_some_and(|draft| !ui.memory(|memory| memory.has_focus(draft.widget))) {
        GraphPropertyDraft::commit(app);
    }
}

// Keep editable graph attribute values unchanged: only their captions follow
// the Hebrew reading edge. Latin identifiers, URLs and free-form text remain
// under TextEdit control rather than being forcibly reversed.
fn attribute_label_layout(rtl: bool) -> egui::Layout {
    if rtl { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) }
}

fn attribute_label(ui: &mut egui::Ui, text: &str, rtl: bool) -> egui::Response {
    let size = egui::vec2(ui.available_width(), ui.text_style_height(&egui::TextStyle::Body));
    ui.allocate_ui_with_layout(size, attribute_label_layout(rtl), |ui| ui.label(text)).inner
}

#[cfg(test)]
mod hebrew_attribute_label_tests {
    use super::*;

    fn label_geometry(rtl: bool) -> (egui::Rect, egui::Rect) {
        let ctx = egui::Context::default();
        let mut actual = None;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 300.0));
        let mut frame = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            let row = ui.available_rect_before_wrap();
            let label = attribute_label(ui, "Identifier", rtl);
            actual = Some((label.rect, row));
        });
        frame.textures_delta.clear();
        actual.expect("label rendered")
    }

    #[test]
    fn graph_attributes_label_follows_hebrew_reading_edge_without_changing_field_data() {
        let (left, left_row) = label_geometry(false);
        let (right, right_row) = label_geometry(true);
        assert!((left_row.left() - right_row.left()).abs() < 1.0);
        assert!((left_row.right() - right_row.right()).abs() < 1.0);
        assert!(left.left() < left_row.center().x, "LTR caption must remain at the left reading edge");
        assert!(right.right() > right_row.center().x, "Hebrew caption must be at the right reading edge");
        assert!(right.center().x - left.center().x > 100.0, "captions must visibly mirror across inspector width");
    }
}
