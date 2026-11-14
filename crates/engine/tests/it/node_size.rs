//! Issue #107: parent size, node output size and independent width/height.
use materialcraft_engine::{Session, core::NodeId, execute, new_session};
use serde_json::{Value as Json, json};

fn command(session: &mut Session, name: &str, parameters: Json) -> Json {
    execute(session, name, &parameters).unwrap()
}

fn node(session: &mut Session, definition: &str) -> NodeId {
    NodeId(command(session, "graph.node.add", json!({"def": definition}))["id"].as_u64().unwrap())
}

fn stored(session: &Session, id: NodeId) -> [i32; 2] {
    session.graph().unwrap().node(id).unwrap().base.size
}

fn full(session: &Session, id: NodeId) -> (u32, u32) {
    let cooked = session.cook_full(session.graph);
    let image = cooked.image(id, 0).unwrap();
    (image.w, image.h)
}

fn dims(session: &Session, id: NodeId) -> (u32, u32) {
    let image = session.cooked.image(id, 0).unwrap();
    (image.w, image.h)
}

#[test]
fn nodes_follow_the_parent_size_and_the_preview_cap_is_only_a_preview() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    session.cook();
    assert_eq!(dims(&session, id), (512, 512), "editing previews are capped");
    let full = session.cook_full(0);
    let image = full.image(id, 0).unwrap();
    assert_eq!((image.w, image.h), (2048, 2048));
    session.preview_size = None;
    session.cook();
    assert_eq!(dims(&session, id), (2048, 2048));
}

#[test]
fn width_and_height_are_independent_axes() {
    let mut session = new_session();
    session.preview_size = None;
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [1, 0]}));
    session.cook();
    assert_eq!(dims(&session, id), (4096, 2048));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [1, -1]}));
    session.cook();
    assert_eq!(dims(&session, id), (4096, 1024));
}

#[test]
fn switching_to_absolute_keeps_the_current_resolution() {
    let mut session = new_session();
    session.preview_size = None;
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [1, 0]}));
    session.cook();
    assert_eq!(dims(&session, id), (4096, 2048));

    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    session.cook();
    assert_eq!(dims(&session, id), (4096, 2048), "stored size must be the log2 of the resolution");
    assert_eq!(session.graph().unwrap().node(id).unwrap().base.size, [12, 11]);
}

#[test]
fn switching_back_to_relative_keeps_the_resolution() {
    let mut session = new_session();
    session.preview_size = None;
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [8, 8]}));
    session.cook();
    assert_eq!(dims(&session, id), (256, 256));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "relativeToParent"}));
    session.cook();
    assert_eq!(dims(&session, id), (256, 256), "the resolution is kept");
    assert_eq!(stored(&session, id), [-3, -3], "as the log2 ratio to the parent");
}

#[test]
fn an_explicit_value_wins_over_the_conversion() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    session.cook();
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [8, 8], "inheritance": "absolute"}));
    assert_eq!(full(&session, id), (256, 256));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [1, 0], "inheritance": "relativeToParent"}));
    assert_eq!(full(&session, id), (4096, 2048));
}

#[test]
fn a_stale_cook_is_not_trusted() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    session.cook();
    command(&mut session, "graph.size", json!({"width": 256, "height": 256}));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    assert_eq!(full(&session, id), (256, 256));
}

#[test]
fn another_graphs_cook_is_not_trusted() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    session.cook();
    command(&mut session, "graph.duplicate", json!({}));
    command(&mut session, "graph.size", json!({"width": 128, "height": 128}));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    assert_eq!(full(&session, id), (128, 128));
}

fn blur_fed_by_a_256_px_source(session: &mut Session) -> NodeId {
    command(session, "graph.size", json!({"width": 2048, "height": 2048}));
    let source = node(session, "uniform");
    command(session, "graph.node.base", json!({"id": source, "param": "outputsize", "inheritance": "absolute"}));
    command(session, "graph.node.base", json!({"id": source, "param": "outputsize", "value": [8, 8]}));
    let blur = node(session, "blur");
    command(session, "graph.connect", json!({"from": source.0, "to": blur.0}));
    blur
}

#[test]
fn an_uncooked_relative_to_input_node_keeps_its_input_derived_size() {
    let mut session = new_session();
    let blur = blur_fed_by_a_256_px_source(&mut session);
    assert_eq!(full(&session, blur), (256, 256));
    command(&mut session, "graph.node.base", json!({"id": blur, "param": "outputsize", "inheritance": "absolute"}));
    assert_eq!(full(&session, blur), (256, 256));
    assert_eq!(stored(&session, blur), [8, 8]);
}

#[test]
fn absolute_back_to_relative_to_input_is_relative_to_the_input() {
    let mut session = new_session();
    let blur = blur_fed_by_a_256_px_source(&mut session);
    command(&mut session, "graph.node.base", json!({"id": blur, "param": "outputsize", "inheritance": "absolute"}));
    command(&mut session, "graph.node.base", json!({"id": blur, "param": "outputsize", "value": [10, 9]}));
    command(&mut session, "graph.node.base", json!({"id": blur, "param": "outputsize", "inheritance": "relativeToInput"}));
    assert_eq!(stored(&session, blur), [2, 1]);
    assert_eq!(full(&session, blur), (1024, 512));
}

#[test]
fn the_legacy_size_mode_converts_too() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "sizeMode": "absolute"}));
    assert_eq!(stored(&session, id), [11, 11]);
    assert_eq!(full(&session, id), (2048, 2048));
    command(&mut session, "graph.node.base", json!({"id": id, "sizeMode": "absolute", "size": [8, 8]}));
    assert_eq!(full(&session, id), (256, 256));
}

#[test]
fn extreme_sizes_are_clamped_both_ways() {
    let mut session = new_session();
    command(&mut session, "graph.size", json!({"width": 16, "height": 8192}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [-12, 12]}));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    assert_eq!(stored(&session, id), [4, 13]);
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "relativeToParent"}));
    assert_eq!(stored(&session, id), [0, 0]);
}

#[test]
fn a_mode_switch_is_one_undo_step() {
    let mut session = new_session();
    session.preview_size = None;
    command(&mut session, "graph.size", json!({"width": 2048, "height": 2048}));
    let id = node(&mut session, "uniform");
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "value": [1, 0]}));
    command(&mut session, "edit.finish", json!({}));
    command(&mut session, "graph.node.base", json!({"id": id, "param": "outputsize", "inheritance": "absolute"}));
    assert_eq!(stored(&session, id), [12, 11]);
    command(&mut session, "edit.undo", json!({}));
    assert_eq!(stored(&session, id), [1, 0]);
}
