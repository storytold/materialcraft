use super::*;

#[test]
fn recent_nodes_are_newest_first_deduplicated_and_capped() {
    let mut ui = UiState::default();
    for i in 0..MAX_RECENT_NODES + 5 {
        ui.add_recent_node(&format!("n{i}"));
    }
    assert_eq!(ui.recent_nodes.len(), MAX_RECENT_NODES);
    assert_eq!(ui.recent_nodes[0], format!("n{}", MAX_RECENT_NODES + 4));
    ui.add_recent_node("n10");
    assert_eq!(ui.recent_nodes[0], "n10");
    assert_eq!(ui.recent_nodes.iter().filter(|n| *n == "n10").count(), 1);
}

#[test]
fn files_without_recent_nodes_still_load_and_the_list_round_trips() {
    let old: UiState = serde_json::from_value(serde_json::json!({"librarySearch": "x"})).unwrap();
    assert!(old.recent_nodes.is_empty());
    let mut ui = UiState::default();
    ui.add_recent_node("blur");
    let back: UiState = serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
    assert_eq!(back.recent_nodes, vec!["blur"]);
}
