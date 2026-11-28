//! Escape clears what was typed first; a second Escape closes the popup.
use egui::{Event, Key};
use egui_kittest::kittest::Queryable;

fn popup_open(h: &egui_kittest::Harness<'static, materialcraft_ui_egui::MaterialCraftApp>) -> bool {
    h.query_by_label("Search nodes").is_some()
}

#[test]
fn first_escape_clears_the_text_and_second_closes_the_popup() {
    let mut h = super::harness();
    h.event(Event::Text("blur".into()));
    h.step();
    h.run_steps(3);
    super::save_snapshot(&mut h, "escape-typed");

    h.key_press(Key::Escape);
    h.step();
    h.run_steps(3);
    super::save_snapshot(&mut h, "escape-cleared");

    assert!(popup_open(&h), "the first Escape must keep the popup open");
    assert!(super::label_position(&h, "Bitmap").is_some(), "the full list is back");

    // The box keeps focus: the next keystrokes land in it, not in the graph
    h.event(Event::Text("grad".into()));
    h.step();
    h.run_steps(3);
    assert!(super::label_position(&h, "Bitmap").is_none(), "typing after Escape filters the list again");
    assert!(h.get_by_label("Search nodes").is_focused(), "the search box keeps focus");
    super::save_snapshot(&mut h, "escape-retyped");

    h.key_press(Key::Escape);
    h.step();
    h.run_steps(3);
    assert!(popup_open(&h), "clearing the retyped text keeps the popup open");

    h.key_press(Key::Escape);
    h.step();
    h.run_steps(3);

    assert!(!popup_open(&h), "the second Escape closes the popup");
}

#[test]
fn escape_in_the_library_filter_clears_it() {
    let mut h = super::harness();
    // Close the node search popup so the library filter can take focus
    h.key_press(Key::Escape);
    h.step();
    h.run_steps(3);
    h.get_by_label("Search the library").click();
    h.step();
    h.event(Event::Text("blur".into()));
    h.step();
    h.run_steps(2);
    assert_eq!(h.state().ui.library_search, "blur");
    super::save_snapshot(&mut h, "library-typed");

    h.key_press(Key::Escape);
    h.step();
    h.run_steps(3);
    super::save_snapshot(&mut h, "library-cleared");

    assert_eq!(h.state().ui.library_search, "");
    assert!(h.get_by_label("Search the library").is_focused(), "the library filter keeps focus");

    // The next keystrokes go into the same field
    h.event(Event::Text("grad".into()));
    h.step();
    h.run_steps(3);
    assert_eq!(h.state().ui.library_search, "grad");
    super::save_snapshot(&mut h, "library-retyped");
}
