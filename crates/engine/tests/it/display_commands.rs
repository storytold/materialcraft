use std::sync::Arc;

use materialcraft_engine::{DisplaySnapshot, commands, execute, geom, new_session};
use serde_json::json;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("materialcraft-display-command-{}-{name}", std::process::id()))
}

#[test]
fn displayed_actions_are_registered_and_require_a_snapshot() {
    let mut session = new_session();
    for id in ["view.3d.saveRender", "view.3d.exportGeometry", "view.2d.copyImage"] {
        assert!(commands::find(id).is_some(), "{id} is missing from the shared command catalog");
        assert!(commands::list(&session, Some(id)).as_array().is_some_and(|commands| commands.iter().any(|command| command["id"] == id)));
        assert!(execute(&mut session, id, &json!({"path": path("missing")})).is_err(), "{id} accepted a missing snapshot");
    }
}

#[test]
fn render_command_writes_the_captured_rgba_after_the_view_changes() {
    let mut session = new_session();
    let pixels = Arc::new(vec![3, 17, 29, 255, 200, 101, 50, 128]);
    let saved = path("frame.png");
    execute(&mut session, "view.3d", &json!({"scene": "sphere"})).unwrap();
    session.begin_display_handoff(DisplaySnapshot::render(2, 1, pixels.clone()));

    let result = execute(&mut session, "view.3d.saveRender", &json!({"path": saved})).unwrap();
    assert_eq!((result["width"].as_u64(), result["height"].as_u64()), (Some(2), Some(1)));
    assert!(session.finish_display_handoff().is_none());
    assert_eq!(Arc::strong_count(&pixels), 1, "the command retained a frame after writing it");
    let written = image::load_from_memory(&std::fs::read(&saved).unwrap()).unwrap().to_rgba8();
    assert_eq!((written.width(), written.height(), written.as_raw()), (2, 1, pixels.as_ref()));
    std::fs::remove_file(saved).unwrap();

    let invalid = path("invalid.png");
    session.begin_display_handoff(DisplaySnapshot::render(2, 1, Arc::new(vec![1, 2, 3])));
    assert!(execute(&mut session, "view.3d.saveRender", &json!({"path": invalid})).is_err());
    assert!(session.finish_display_handoff().is_none());
    assert!(!invalid.exists(), "invalid RGBA wrote a partial file");

    session.begin_display_handoff(DisplaySnapshot::render(8193, 1, Arc::new(vec![0; 8193 * 4])));
    let error = execute(&mut session, "view.3d.saveRender", &json!({"path": invalid})).unwrap_err();
    assert!(error.to_string().contains("dimensions"), "{error}");
    assert!(!invalid.exists(), "an over-limit frame wrote a partial file");
}

#[test]
fn geometry_command_writes_only_the_captured_mesh() {
    let mut session = new_session();
    let mesh = Arc::new(geom::primitives::cube(1.0));
    let expected = geom::primitives::sphere(1.0, 8, 4);
    let saved = path("mesh.obj");
    session.begin_display_handoff(DisplaySnapshot::geometry(mesh.clone()));
    session.custom_mesh = Some(("newer".into(), Arc::new(expected)));

    execute(&mut session, "view.3d.exportGeometry", &json!({"path": saved})).unwrap();
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), materialcraft_engine::formats::mesh::obj_text(&mesh).unwrap());
    assert!(session.finish_display_handoff().is_none());
    assert_eq!(Arc::strong_count(&mesh), 1, "the command retained a mesh after writing it");
    std::fs::remove_file(saved).unwrap();

    let mut invalid = geom::primitives::cube(1.0);
    invalid.positions[0].x = f32::INFINITY;
    session.begin_display_handoff(DisplaySnapshot::geometry(Arc::new(invalid)));
    let invalid_path = path("invalid-mesh.obj");
    assert!(execute(&mut session, "view.3d.exportGeometry", &json!({"path": invalid_path})).is_err());
    assert!(!invalid_path.exists(), "non-finite geometry wrote a partial file");
}

#[test]
fn copy_command_returns_the_same_display_buffer_and_alpha_mode() {
    let mut session = new_session();
    let pixels = Arc::new(vec![64, 0, 0, 128]);
    session.begin_display_handoff(DisplaySnapshot::image(1, 1, pixels.clone(), true));

    let result = execute(&mut session, "view.2d.copyImage", &json!({})).unwrap();
    let output = session.finish_display_handoff().and_then(DisplaySnapshot::into_image).unwrap();
    assert_eq!((result["width"].as_u64(), result["height"].as_u64(), result["premultiplied"].as_bool()), (Some(1), Some(1), Some(true)));
    assert!(Arc::ptr_eq(&pixels, &output.2), "the command copied the displayed RGBA buffer");
    assert_eq!(output.2.as_slice(), [64, 0, 0, 128]);
    drop(output);
    assert_eq!(Arc::strong_count(&pixels), 1);
}
