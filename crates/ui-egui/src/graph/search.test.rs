use super::*;

#[test]
fn result_count_reads_shown_over_all() {
    assert_eq!(result_count(3, 120), "3 / 120");
    assert_eq!(result_count(0, 120), "0 / 120");
}

#[test]
fn the_total_is_the_whole_searchable_catalog() {
    let total = matches("").len();
    assert!(total > 10, "empty search must list every node");
    assert!(matches("blur").len() < total);
    assert!(matches("blur").iter().all(|d| d.id != "graph_instance"));
}
