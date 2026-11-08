//! Nodes picked in the search come first the next time it opens with an empty box.
use egui::{Event, Key};

#[test]
fn a_picked_node_is_listed_first_when_the_search_reopens() {
    let mut h = super::harness();
    let bitmap = super::label_position(&h, "Bitmap").expect("first result");
    let before = super::label_position(&h, "Blur").map(|p| p.y);
    super::save_snapshot(&mut h, "recent-before");
    h.event(Event::Text("blur".into()));
    h.step();
    h.key_press(Key::Enter);
    h.step();
    h.run_steps(3);
    assert_eq!(h.state().ui.recent_nodes, vec!["blur"]);
    h.key_press(Key::Tab);
    h.step();
    h.run_steps(5);
    super::save_snapshot(&mut h, "recent-after");
    let blur = super::label_position(&h, "Blur").expect("Blur is listed");
    let bitmap_after = super::label_position(&h, "Bitmap").expect("Bitmap is listed");
    assert!(blur.y < bitmap_after.y, "the recent node must be above the catalog start");
    assert!(before.is_none_or(|y| y > bitmap.y), "Blur was below Bitmap before");
}
