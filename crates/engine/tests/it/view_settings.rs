use materialcraft_engine::{execute, new_session};
use serde_json::json;

#[test]
fn rejected_2d_settings_leave_the_view_unchanged() {
    let mut s = new_session();
    let a = execute(&mut s, "graph.node.add", &json!({"def": "channels_shuffle"})).unwrap()["id"].as_u64().unwrap();
    let b = execute(&mut s, "graph.node.add", &json!({"def": "channels_shuffle"})).unwrap()["id"].as_u64().unwrap();
    execute(&mut s, "view.2d", &json!({"node": a, "port": 1, "channel": "g"})).unwrap();
    let before = s.view2d.clone();
    // A bad channel used to leave all the preceding fields changed despite returning an error.
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
    // A valid scene must not be applied when another setting in the request is rejected.
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
