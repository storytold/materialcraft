use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};
use egui_phosphor::regular as icon;

pub(crate) fn section(ui: &mut Ui, id: &str, title: &str, body: impl FnOnce(&mut Ui)) {
    let tokens = Tokens::get(ui.ctx());
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
    crate::widgets::name(&response, egui::Role::Button, title);
    ui.painter().rect_filled(rect, 3.0, tokens.row);
    let rtl = crate::i18n::is_rtl();
    let chevron = if state.is_open() {
        icon::CARET_DOWN
    } else if rtl {
        icon::CARET_LEFT
    } else {
        icon::CARET_RIGHT
    };
    let (chevron_pos, heading_pos, heading_align) = header_geometry(rect, rtl);
    ui.painter().text(chevron_pos, heading_align, chevron, egui::FontId::proportional(10.0), tokens.text_dim);
    ui.painter().text(heading_pos, heading_align, title.to_uppercase(), egui::FontId::proportional(10.5), tokens.text_strong);
    if response.clicked() {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
        ui.add_space(3.0);
        body(ui);
        ui.add_space(6.0);
    });
}

/// Align the inspector's collapsible headings with the current reading edge.
/// This affects only presentation; persisted docking/layout state is untouched.
fn header_geometry(rect: egui::Rect, rtl: bool) -> (egui::Pos2, egui::Pos2, egui::Align2) {
    let (edge, direction, align) =
        if rtl { (rect.right_center(), -1.0, egui::Align2::RIGHT_CENTER) } else { (rect.left_center(), 1.0, egui::Align2::LEFT_CENTER) };
    (edge + vec2(direction * 6.0, 0.0), edge + vec2(direction * 20.0, 0.0), align)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_headings_anchor_to_the_reading_edge() {
        let rect = egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), vec2(240.0, 20.0));
        let (ltr_chevron, ltr_heading, ltr_align) = header_geometry(rect, false);
        let (rtl_chevron, rtl_heading, rtl_align) = header_geometry(rect, true);
        assert_eq!(ltr_align, egui::Align2::LEFT_CENTER);
        assert_eq!(rtl_align, egui::Align2::RIGHT_CENTER);
        assert_eq!(ltr_chevron.x, 16.0);
        assert_eq!(ltr_heading.x, 30.0);
        assert_eq!(rtl_chevron.x, 244.0);
        assert_eq!(rtl_heading.x, 230.0);
        assert_eq!(rtl_chevron.y, ltr_chevron.y);
    }
}
