//! The letters of a result that match what was typed are underlined and recoloured.
use egui::{Event, Rect, epaint::Shape};

fn underlines(shape: &Shape, popup: Rect, label: &str, out: &mut Vec<bool>) {
    match shape {
        Shape::Text(t) if popup.contains(t.pos) && t.galley.text().trim() == label => {
            out.extend(t.galley.job.sections.iter().map(|s| s.format.underline.width > 0.0));
        }
        Shape::Vec(v) => v.iter().for_each(|s| underlines(s, popup, label, out)),
        _ => {}
    }
}

/// Whether each styled section of the result row `label` inside the search popup is underlined
fn underlines_of(h: &egui_kittest::Harness<'static, materialcraft_ui_egui::MaterialCraftApp>, label: &str) -> Vec<bool> {
    let mut out = Vec::new();
    if let Some(popup) = h.ctx.memory(|m| m.area_rect(egui::Id::unique("node-search"))) {
        h.output().shapes.iter().for_each(|s| underlines(&s.shape, popup, label, &mut out));
    }
    out
}

#[test]
fn typed_letters_are_marked_and_the_rest_is_not() {
    let mut h = super::harness();
    let before = underlines_of(&h, "Bitmap");
    assert!(!before.is_empty() && before.iter().all(|u| !u), "nothing is marked before typing: {before:?}");
    h.event(Event::Text("blu".into()));
    h.step();
    h.run_steps(3);
    super::save_snapshot(&mut h, "highlight-blu");
    let after = underlines_of(&h, "Blur");
    assert!(after.contains(&true) && after.contains(&false), "matched and unmatched letters look the same: {after:?}");
}
