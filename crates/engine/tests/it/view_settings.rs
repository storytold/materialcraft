use materialcraft_engine::{execute, new_session};
use serde_json::json;

#[test]
fn rejected_2d_settings_leave_the_view_unchanged() {
    let mut s = new_session();
    let a = execute(&mut s, "graph.node.add", &json!({"def": "channels_shuffle"})).unwrap()["id"].as_u64().unwrap();
    let b = execute(&mut s, "graph.node.add", &json!({"def": "channels_shuffle"})).unwrap()["id"].as_u64().unwrap();
    execute(&mut s, "view.2d", &json!({"node": a, "port": 1, "channel": "g"})).unwrap();
    let before = s.view2d.clone();

    // A bad channel used to leave all the preceding fields changed despite returning an error
    assert!(execute(&mut s, "view.2d", &json!({"node": b, "port": 2, "tiled": true, "histogram": true, "channel": "invalid"})).is_err());
    assert_eq!(s.view2d, before);
    assert!(execute(&mut s, "view.2d", &json!({"node": u64::MAX, "channel": "b", "tiled": true})).is_err());
    assert_eq!(s.view2d, before);
    assert_eq!(execute(&mut s, "session.inspect", &json!({})).unwrap()["view2d"], serde_json::to_value(&before).unwrap());
}

#[test]
fn rejected_3d_settings_leave_the_view_unchanged() {
    let mut s = new_session();
    execute(&mut s, "view.3d", &json!({"scene": "sphere", "environment": "sunset", "yaw": 70})).unwrap();
    let before = s.view3d.clone();

    // A valid scene must not be applied when another setting in the request is rejected
    for params in [
        json!({"scene": "cube", "environment": "invalid", "yaw": 10}),
        json!({"scene": "invalid", "environment": "studio"}),
        json!({"scene": "custom", "environment": "studio"}),
        json!({"scene": "cube", "target": [1, 2]}),
    ] {
        assert!(execute(&mut s, "view.3d", &params).is_err(), "{params}");
        assert_eq!(s.view3d, before, "{params}");
    }
    assert_eq!(execute(&mut s, "session.inspect", &json!({})).unwrap()["view3d"], serde_json::to_value(&before).unwrap());
}

#[test]
fn valid_view_updates_still_apply_and_return_the_complete_settings() {
    let mut s = new_session();
    let node = execute(&mut s, "graph.node.add", &json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap();
    let result = execute(&mut s, "view.2d", &json!({"node": node, "channel": "r", "tiled": true, "histogram": true})).unwrap();
    assert_eq!(result["node"], node);
    assert_eq!(result["channel"], "r");
    assert_eq!(result["tiled"], true);
    assert_eq!(result["histogram"], true);
    assert_eq!(result, serde_json::to_value(&s.view2d).unwrap());
    let result = execute(&mut s, "view.3d", &json!({"scene": "cube", "environment": "night", "yaw": 370, "target": [1, 2, 3]})).unwrap();
    assert_eq!(result["scene"], "cube");
    assert_eq!(result["environment"], "night");
    assert_eq!(result["yaw"], 10.0);
    assert_eq!(result["target"], json!([1.0, 2.0, 3.0]));
    assert_eq!(result, serde_json::to_value(&s.view3d).unwrap());
}

#[test]
fn the_2d_view_starts_showing_srgb_values_of_all_channels_with_the_zoom_kept() {
    let session = new_session();
    let view = serde_json::to_value(&session.view2d).unwrap();

    assert_eq!(view["channel"], "rgba");
    assert_eq!(view["srgb"], true);
    assert_eq!(view["maintainZoom"], true);
    assert_eq!(view["histogramChannel"], "luminance");
    assert_eq!(view["histogramBits"], 0);
    for off in ["tiled", "histogram", "information", "checkerboard", "premultiplied", "physicalSize", "uv", "histogramSqrt"] {
        assert_eq!(view[off], false, "{off}");
    }
    for unset in ["sampler", "histogramSelection", "background"] {
        assert!(view[unset].is_null(), "{unset}");
    }
}

#[test]
fn every_2d_display_setting_applies_and_shows_in_inspect() {
    let mut session = new_session();
    let node = execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap();
    let settings = json!({
        "node": node,
        "checkerboard": true,
        "premultiplied": true,
        "srgb": false,
        "physicalSize": true,
        "maintainZoom": false,
        "uv": true,
        "information": true,
        "sampler": [0.25, 0.75],
        "histogramChannel": "rgb",
        "histogramBits": 8,
        "histogramSqrt": true,
        "histogramSelection": [0.75, 0.25],
        "background": "backdrop.png",
    });

    let result = execute(&mut session, "view.2d", &settings).unwrap();

    for (key, expected) in settings.as_object().unwrap() {
        // A selection is stored low end first, whichever way it was dragged
        let expected = if key == "histogramSelection" { json!([0.25, 0.75]) } else { expected.clone() };
        assert_eq!(result[key], expected, "{key}");
    }
    assert_eq!(result, serde_json::to_value(&session.view2d).unwrap());
    assert_eq!(execute(&mut session, "session.inspect", &json!({})).unwrap()["view2d"], result);
}

#[test]
fn a_rejected_2d_display_setting_changes_nothing() {
    let mut session = new_session();
    execute(&mut session, "view.2d", &json!({"sampler": [0.5, 0.5], "histogramSelection": [0.2, 0.4], "background": "backdrop.png"})).unwrap();
    let before = session.view2d.clone();

    for rejected in [
        json!({"srgb": false, "histogramChannel": "cyan"}),
        json!({"tiled": true, "histogramBits": 12}),
        json!({"tiled": true, "histogramBits": "16"}),
        json!({"uv": true, "sampler": [0.5]}),
        json!({"uv": true, "sampler": "center"}),
        json!({"uv": true, "sampler": [0.5, "half"]}),
        json!({"checkerboard": true, "histogramSelection": [0.1, 0.2, 0.3]}),
        json!({"checkerboard": true, "background": ""}),
        json!({"checkerboard": true, "background": 7}),
    ] {
        assert!(execute(&mut session, "view.2d", &rejected).is_err(), "{rejected}");
        assert_eq!(session.view2d, before, "{rejected}");
    }
}

#[test]
fn the_sampler_and_the_histogram_selection_stay_inside_the_image_and_null_removes_them() {
    let mut session = new_session();

    let result = execute(&mut session, "view.2d", &json!({"sampler": [-3.0, 9.0], "histogramSelection": [2.0, -1.0], "background": "backdrop.png"})).unwrap();
    assert_eq!(result["sampler"], json!([0.0, 1.0]));
    assert_eq!(result["histogramSelection"], json!([0.0, 1.0]));

    let result = execute(&mut session, "view.2d", &json!({"sampler": null, "histogramSelection": null, "background": null})).unwrap();
    assert!(result["sampler"].is_null() && result["histogramSelection"].is_null() && result["background"].is_null());

    // Histogram bits of 0 go back to following the image
    execute(&mut session, "view.2d", &json!({"histogramBits": 32})).unwrap();
    assert_eq!(execute(&mut session, "view.2d", &json!({"histogramBits": 0})).unwrap()["histogramBits"], 0);
}

#[test]
fn every_3d_view_setting_applies_and_shows_in_inspect() {
    let mut session = new_session();
    let settings = json!({
        "scene": "cloth",
        "uvScale": [2.0, 3.0],
        "physicalSize": true,
        "focalLength": 50.0,
        "projection": "orthographic",
        "background": "environment",
        "backgroundColor": [0.25, 0.5, 0.75],
        "overrideResolution": true,
        "viewportResolution": false,
        "renderResolution": [1280, 720],
        "renderScale": 50,
        "autoClip": false,
        "nearClip": 5.0,
        "farClip": 900.0,
        "envEnabled": false,
        "wireframe": true,
        "backfaceCulling": false,
        "boundingBox": true,
        "lightHelpers": true,
        "vertexTangentSpace": true,
        "shader": "metallicRoughnessCoated",
        "technique": "parallax",
        "meshInfo": "uvTile",
        "tessellation": 2.0,
        "samples": 3,
        "ambientOcclusion": 8,
        "shadows": true,
        "displacement": false,
        "viewChannels": "g",
        "colorSpace": "srgb",
    });

    let result = execute(&mut session, "view.3d", &settings).unwrap();

    for (key, expected) in settings.as_object().unwrap() {
        assert_eq!(&result[key], expected, "{key}");
    }
    assert_eq!(execute(&mut session, "session.inspect", &json!({})).unwrap()["view3d"], result);

    // The overlays are named for what they show; the view keeps them as "show…"
    let result = execute(&mut session, "view.3d", &json!({"axis": true, "grid": true, "resolution": true, "sceneStats": true, "fps": true})).unwrap();
    for shown in ["showAxis", "showGrid", "showResolution", "sceneStats", "fps"] {
        assert_eq!(result[shown], true, "{shown}");
    }
}

#[test]
fn lights_and_material_parameters_change_part_by_part() {
    let mut session = new_session();

    let result = execute(
        &mut session,
        "view.3d",
        &json!({"ambient": {"color": [0.125, 0.25, 0.5]}, "light1": {"enabled": true, "intensity": 99.0}, "light2": {"yaw": 400.0}}),
    )
    .unwrap();
    assert_eq!(result["ambient"], json!({"enabled": true, "color": [0.125, 0.25, 0.5]}));
    assert_eq!((&result["lights"][0]["enabled"], &result["lights"][0]["intensity"]), (&json!(true), &json!(10.0)), "the intensity is held in range");
    assert_eq!(result["lights"][1]["yaw"], 40.0);
    assert_eq!(result["lights"][1]["enabled"], false);

    // A value is kept only while it differs from its default; null puts the default back
    let result = execute(
        &mut session,
        "view.3d",
        &json!({"material": {"coatWeight": 0.5, "baseColor": [1.0, 0.0, 0.0], "specularRoughness": 0.3, "geometryThinWalled": true}}),
    )
    .unwrap();
    assert_eq!(result["material"], json!({"baseColor": [1.0, 0.0, 0.0], "coatWeight": 0.5, "geometryThinWalled": true}));
    let result = execute(
        &mut session,
        "view.3d",
        &json!({"material": {"coatWeight": null, "baseColor": [5.0, -1.0, 0.5]}, "channels": {"baseColor": false, "height": false}}),
    )
    .unwrap();
    assert_eq!(result["material"], json!({"baseColor": [1.0, 0.0, 0.5], "geometryThinWalled": true}));
    assert_eq!(result["disabled"], json!(["baseColor", "height"]));
    assert_eq!(execute(&mut session, "view.3d", &json!({"channels": {"height": true}})).unwrap()["disabled"], json!(["baseColor"]));
}

#[test]
fn a_rejected_3d_view_setting_changes_nothing() {
    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"material": {"coatWeight": 0.5}, "light1": {"enabled": true}})).unwrap();
    let before = session.view3d.clone();

    for rejected in [
        json!({"wireframe": true, "projection": "fisheye"}),
        json!({"wireframe": true, "background": 3}),
        json!({"wireframe": true, "shader": "toon"}),
        json!({"wireframe": true, "colorSpace": "rec2020"}),
        json!({"wireframe": true, "renderScale": 75}),
        json!({"wireframe": true, "backgroundColor": [0.1, 0.2]}),
        json!({"wireframe": true, "backgroundColor": [0.1, "invalid", 0.2, 0.3]}),
        json!({"wireframe": true, "target": [1.0, "invalid", 2.0, 3.0]}),
        json!({"wireframe": true, "uvScale": "double"}),
        json!({"wireframe": true, "uvScale": [2.0, "invalid", 3.0]}),
        json!({"wireframe": true, "renderResolution": [1920]}),
        json!({"wireframe": true, "renderResolution": [1920, "invalid", 1080]}),
        json!({"wireframe": true, "ambient": true}),
        json!({"wireframe": true, "light1": {"color": [1.0]}}),
        json!({"wireframe": true, "material": {"noSuchParameter": 1.0}}),
        json!({"wireframe": true, "material": {"baseColor": 0.5}}),
        json!({"wireframe": true, "material": {"coatWeight": [1.0, 0.0, 0.0]}}),
        json!({"wireframe": true, "channels": {"baseColor": "off"}}),
        json!({"wireframe": true, "channels": {"noSuchParameter": false}}),
    ] {
        assert!(execute(&mut session, "view.3d", &rejected).is_err(), "{rejected}");
        assert_eq!(session.view3d, before, "{rejected}");
    }
}

#[test]
fn the_camera_commands_turn_frame_and_reset_the_camera() {
    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"distance": 9.0, "target": [0.2, 0.0, 0.0], "focalLength": 70.0, "exposure": 1.0})).unwrap();

    let top = execute(&mut session, "view.3d.viewpoint", &json!({"name": "top"})).unwrap();
    assert_eq!((&top["yaw"], &top["pitch"], &top["distance"], &top["target"]), (&json!(0.0), &json!(89.0), &json!(9.0), &json!([0.0, 0.0, 0.0])));
    assert!(execute(&mut session, "view.3d.viewpoint", &json!({"name": "underneath"})).is_err());
    assert!(execute(&mut session, "view.3d.viewpoint", &json!({})).is_err());

    // A longer lens frames the mesh from further away
    let framed = execute(&mut session, "view.3d.frame", &json!({})).unwrap()["distance"].as_f64().unwrap();
    execute(&mut session, "view.3d", &json!({"focalLength": 35.0})).unwrap();
    assert!(execute(&mut session, "view.3d.frame", &json!({})).unwrap()["distance"].as_f64().unwrap() < framed);

    let square_distance = session.view3d.distance;
    let portrait_distance = execute(&mut session, "view.3d.frame", &json!({"aspect": 0.5})).unwrap()["distance"].as_f64().unwrap();
    assert!(portrait_distance > f64::from(square_distance), "a narrow viewport needs more distance to fit the mesh");

    execute(&mut session, "view.3d", &json!({"focalLength": 70.0, "yaw": 200.0})).unwrap();
    let position = execute(&mut session, "view.3d.resetCamera", &json!({})).unwrap();
    assert_eq!((&position["yaw"], &position["focalLength"], &position["exposure"]), (&json!(35.0), &json!(70.0), &json!(1.0)));
    assert_eq!(execute(&mut session, "view.3d.resetCamera", &json!({"properties": true})).unwrap()["focalLength"], 35.0);
}

#[test]
fn the_resets_put_back_their_own_part_and_a_saved_default_replaces_the_built_in_one() {
    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"light1": {"enabled": true}, "exposure": 2.0, "envEnabled": false, "shader": "blinn", "material": {"baseColor": [1.0, 0.0, 0.0], "glossiness": 0.2}})).unwrap();

    assert_eq!(execute(&mut session, "view.3d.resetLights", &json!({})).unwrap()["lights"][0]["enabled"], false);
    let environment = execute(&mut session, "view.3d.resetEnvironment", &json!({})).unwrap();
    assert_eq!((&environment["exposure"], &environment["envEnabled"], &environment["shader"]), (&json!(0.0), &json!(true), &json!("blinn")));

    // The base color is shared by every shader, the glossiness by two
    let common = execute(&mut session, "view.3d.resetMaterial", &json!({"common": true})).unwrap();
    assert_eq!((&common["shader"], &common["material"]), (&json!("blinn"), &json!({})));
    execute(&mut session, "view.3d", &json!({"material": {"fuzzWeight": 1.0}})).unwrap();
    assert_eq!(
        execute(&mut session, "view.3d.resetMaterial", &json!({"common": true})).unwrap()["material"],
        json!({"fuzzWeight": 1.0}),
        "a parameter of one shader stays"
    );
    let all = execute(&mut session, "view.3d.resetMaterial", &json!({})).unwrap();
    assert_eq!((&all["shader"], &all["material"]), (&json!("openPbr"), &json!({})));

    execute(&mut session, "view.3d", &json!({"scene": "torus", "focalLength": 20.0})).unwrap();
    execute(&mut session, "view.3d.saveDefault", &json!({})).unwrap();
    execute(&mut session, "view.3d", &json!({"scene": "cube", "focalLength": 90.0})).unwrap();
    execute(&mut session, "view.3d.reset", &json!({})).unwrap();
    assert_eq!((session.view3d.scene.as_str(), session.view3d.focal_length), ("torus", 20.0));
}

#[test]
fn reset_uses_a_built_in_scene_when_the_saved_custom_mesh_is_unavailable() {
    let mut session = new_session();
    let mut saved = session.view3d.clone();
    saved.scene = "custom".into();
    saved.focal_length = 42.0;
    session.default_view3d = Some(saved);

    execute(&mut session, "view.3d.reset", &json!({})).unwrap();

    assert_eq!(session.view3d.scene, materialcraft_engine::View3d::default().scene);
    assert_eq!(session.view3d.focal_length, 42.0);
}

#[test]
fn large_render_counts_clamp_before_narrowing() {
    let mut session = new_session();

    let view = execute(&mut session, "view.3d", &json!({"samples": 4_294_967_297_u64, "ambientOcclusion": 4_294_967_296_u64})).unwrap();

    assert_eq!(view["samples"], 4);
    assert_eq!(view["ambientOcclusion"], 32);
}

#[test]
fn a_state_file_brings_the_view_back_and_a_wrong_file_changes_nothing() {
    let directory = std::env::temp_dir().join(format!("materialcraft-view3d-state-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("view.json");
    let path_text = path.to_str().unwrap();

    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"scene": "torus", "focalLength": 60.0, "wireframe": true, "material": {"coatWeight": 0.7}})).unwrap();
    let saved = session.view3d.clone();
    execute(&mut session, "view.3d.saveState", &json!({"path": path_text})).unwrap();

    // Without the mesh, the scene on show stays
    execute(&mut session, "view.3d.reset", &json!({})).unwrap();
    execute(&mut session, "view.3d", &json!({"scene": "sphere"})).unwrap();
    execute(&mut session, "view.3d.loadState", &json!({"path": path_text})).unwrap();
    assert_eq!(session.view3d, materialcraft_engine::View3d { scene: "sphere".into(), ..saved.clone() });
    execute(&mut session, "view.3d.loadState", &json!({"path": path_text, "mesh": true})).unwrap();
    assert_eq!(session.view3d, saved);

    // Values a file should not hold are put in range; a file that is no state is refused
    let hostile = std::fs::read_to_string(&path).unwrap().replace("60.0", "1.0e30").replace("\"torus\"", "\"no_such_mesh\"");
    std::fs::write(&path, hostile).unwrap();
    execute(&mut session, "view.3d.loadState", &json!({"path": path_text, "mesh": true})).unwrap();
    assert_eq!((session.view3d.focal_length, session.view3d.scene.as_str()), (100.0, "torus"));

    let before = session.view3d.clone();
    for wrong in
        ["", "{}", "[1, 2]", r#"{"format": "something-else", "version": 1, "view": {}}"#, r#"{"format": "materialcraft-3d-view", "version": 999, "view": {}}"#]
    {
        std::fs::write(&path, wrong).unwrap();
        assert!(execute(&mut session, "view.3d.loadState", &json!({"path": path_text})).is_err(), "{wrong}");
        assert_eq!(session.view3d, before, "{wrong}");
    }
    assert!(execute(&mut session, "view.3d.loadState", &json!({"path": directory.join("missing.json").to_str().unwrap()})).is_err());
    assert!(execute(&mut session, "view.3d.saveState", &json!({})).is_err());
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_new_package_starts_from_the_saved_default_view() {
    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"scene": "torus", "grid": true})).unwrap();
    execute(&mut session, "view.3d.saveDefault", &json!({})).unwrap();
    execute(&mut session, "view.3d", &json!({"scene": "cube", "grid": false})).unwrap();

    execute(&mut session, "file.newPackage", &json!({})).unwrap();
    assert_eq!((session.view3d.scene.as_str(), session.view3d.show_grid), ("torus", true));
}

#[test]
fn the_physical_size_of_the_graph_sizes_the_material_on_the_mesh() {
    let mut session = new_session();
    execute(&mut session, "view.3d", &json!({"uvScale": [2.0, 1.0]})).unwrap();
    assert_eq!(session.rendered_view3d(), session.view3d, "off, the view renders as it is set");

    // A graph without a physical size leaves the material alone
    execute(&mut session, "view.3d", &json!({"physicalSize": true})).unwrap();
    assert_eq!(session.rendered_view3d().uv_scale, [2.0, 1.0]);

    // The mesh is a meter across: a graph 50 by 200 covers it twice across and half down
    execute(&mut session, "graph.context", &json!({"physicalSize": [50.0, 200.0, 10.0]})).unwrap();
    assert_eq!(session.rendered_view3d().uv_scale, [4.0, 0.5]);
    assert_eq!(session.view3d.uv_scale, [2.0, 1.0], "the setting itself stays");

    execute(&mut session, "view.3d", &json!({"physicalSize": false})).unwrap();
    assert_eq!(session.rendered_view3d().uv_scale, [2.0, 1.0]);
}

#[cfg(unix)]
#[test]
fn view_file_commands_reject_a_named_pipe_without_waiting_for_a_writer() {
    let path = std::env::temp_dir().join(format!("materialcraft-view-fifo-{}", std::process::id()));
    let status = std::process::Command::new("mkfifo").arg(&path).status().unwrap();
    assert!(status.success());

    let mut session = new_session();
    let path_text = path.to_string_lossy();
    for command in ["view.3d.loadState", "view.3d.loadMesh"] {
        let result = execute(&mut session, command, &json!({"path": path_text}));
        assert!(result.is_err(), "{command} accepted a named pipe");
    }

    std::fs::remove_file(path).unwrap();
}
