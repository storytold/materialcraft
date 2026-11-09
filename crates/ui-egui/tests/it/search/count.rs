//! The search popup shows how many nodes match, and nothing before anything is typed.
use egui::{Event, Rect, epaint::Shape};

/// The popup's "shown / all" line as numbers, read from the text drawn inside the popup
fn count_line(h: &egui_kittest::Harness<'static, materialcraft_ui_egui::MaterialCraftApp>) -> Option<(usize, usize)> {
    fn find(shape: &Shape, popup: Rect) -> Option<(usize, usize)> {
        match shape {
            Shape::Text(t) if popup.contains(t.pos) => {
                let (shown, total) = t.galley.text().split_once(" / ")?;
                Some((shown.trim().parse().ok()?, total.trim().parse().ok()?))
            }
            Shape::Vec(v) => v.iter().find_map(|s| find(s, popup)),
            _ => None,
        }
    }
    let popup = h.ctx.memory(|m| m.area_rect(egui::Id::unique("node-search")))?;
    h.output().shapes.iter().find_map(|s| find(&s.shape, popup))
}

fn type_text(h: &mut egui_kittest::Harness<'static, materialcraft_ui_egui::MaterialCraftApp>, text: &str) {
    h.event(Event::Text(text.into()));
    h.step();
    h.run_steps(3);
}

#[test]
fn count_line_appears_once_a_query_is_typed() {
    let mut h = super::harness();
    assert_eq!(count_line(&h), None, "no count before typing");
    type_text(&mut h, "blur");
    super::save_snapshot(&mut h, "count-blur");
    let (shown, total) = count_line(&h).expect("count line");
    assert!(shown >= 1 && shown < total, "{shown} / {total}");
}

#[test]
fn count_reaches_zero_for_gibberish() {
    let mut h = super::harness();
    type_text(&mut h, "qqqzzzxxx");
    super::save_snapshot(&mut h, "count-none");
    assert!(matches!(count_line(&h), Some((0, total)) if total > 0));
}
