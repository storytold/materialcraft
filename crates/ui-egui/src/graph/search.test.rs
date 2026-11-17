use super::*;

fn ids(list: &[&materialcraft_engine::nodes::NodeDefinition]) -> Vec<&'static str> {
    list.iter().map(|d| d.id).collect()
}

fn recent(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

#[test]
fn recent_nodes_come_first_newest_first_when_the_box_is_empty() {
    let all = matches("");
    let first_ids = ids(&all[..2]);
    let picked = recent(&["blur", "bitmap"]);
    let out = with_recent_first(all.clone(), "", &picked);
    assert_eq!(ids(&out[..2]), vec!["blur", "bitmap"]);
    assert_eq!(out.len(), all.len(), "nothing is lost or repeated");
    assert_ne!(first_ids, vec!["blur", "bitmap"], "the catalog order differs, so this test shows the reorder");
}

#[test]
fn a_typed_query_keeps_its_own_ranking() {
    let list = matches("blur");
    let out = with_recent_first(list.clone(), "blur", &recent(&["motion_blur"]));
    assert_eq!(ids(&out), ids(&list));
}

#[test]
fn unknown_and_duplicate_ids_are_ignored() {
    let all = matches("");
    let out = with_recent_first(all.clone(), "", &recent(&["no_such_node", "blur", "blur", ""]));
    assert_eq!(out[0].id, "blur");
    assert_eq!(out.iter().filter(|d| d.id == "blur").count(), 1);
    assert_eq!(out.len(), all.len());
}

#[test]
fn a_hostile_long_list_is_capped() {
    let all = matches("");
    let many: Vec<String> = all.iter().map(|d| d.id.to_string()).cycle().take(10_000).collect();
    assert_eq!(with_recent_first(all.clone(), "", &many).len(), all.len());
}
