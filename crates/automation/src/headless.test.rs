#[cfg(test)]
mod end_merge_tests {
    use crate::headless::{Filesystem, Headless, RenderOptions};
    use serde_json::json;

    #[test]
    fn linked_material_source_opens_as_sbs_with_a_mcraft_name() {
        use materialcraft_engine::{Session, core::Package, file_cmds};

        let dir = tempfile::tempdir().unwrap();
        let mut source = Headless::new(Session::new(Package::new("Linked source")), Filesystem::TrustedLocal);
        source.command_run("file.new", json!({"template": "empty"})).unwrap();
        source.command_run("graph.node.add", json!({"def": "uniform"})).unwrap();
        let xml = file_cmds::save_bytes(&source.session).unwrap();
        let archive = file_cmds::save_files_at(&source.session, "archive.mcraft").unwrap().zip().unwrap();

        for (name, bytes) in [("source.sbs", &xml), ("source.mcraft", &xml), ("archive.mcraft", &archive)] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let mut opened = Headless::new(Headless::default_session(), Filesystem::TrustedLocal);
            opened.load(path.to_str().unwrap()).unwrap_or_else(|error| panic!("{name}: {error}"));
            let graph = opened.session.graph().unwrap();
            assert_eq!(graph.nodes.len(), 1, "{name}");
            assert_eq!(graph.nodes[0].def, "uniform", "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn trusted_reads_are_bounded_and_regular_files_only() {
        let dir = std::env::temp_dir().join(format!("materialcraft-read-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("small.txt");
        std::fs::write(&small, b"ok").unwrap();
        assert_eq!(crate::headless::read_file(&Filesystem::TrustedLocal, small.to_str().unwrap()).unwrap(), b"ok");

        let directory = dir.join("directory");
        std::fs::create_dir(&directory).unwrap();
        let err = crate::headless::read_file(&Filesystem::TrustedLocal, directory.to_str().unwrap()).unwrap_err().to_string();
        assert!(err.contains("not a regular file"), "{err}");

        let huge = dir.join("huge.bin");
        std::fs::File::create(&huge).unwrap().set_len(crate::workspace::MAX_READ_BYTES + 1).unwrap();
        let err = crate::headless::read_file(&Filesystem::TrustedLocal, huge.to_str().unwrap()).unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn trusted_reads_reject_named_pipes_without_waiting_for_a_writer() {
        use std::os::unix::fs::OpenOptionsExt;
        use std::time::Duration;

        let path = std::env::temp_dir().join(format!("materialcraft-read-pipe-{}", std::process::id()));
        assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
        let reader_path = path.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let result = crate::headless::read_file(&Filesystem::TrustedLocal, reader_path.to_str().unwrap());
            sender.send(result).unwrap();
        });

        let result = receiver.recv_timeout(Duration::from_secs(2));
        if result.is_err() {
            // Release a blocked reader so a regression cannot strand the test process
            let writer = std::fs::OpenOptions::new().write(true).custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32).open(&path).unwrap();
            drop(writer);
        }
        reader.join().unwrap();
        std::fs::remove_file(path).unwrap();

        let error = result.expect("a named pipe must be rejected before waiting for a writer").unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[test]
    fn two_param_requests_are_two_undo_steps() {
        let mut h = Headless::new(Headless::default_session(), Filesystem::TrustedLocal);
        h.command_run("file.new", json!({"template": "empty"})).unwrap();
        let id = h.command_run("graph.node.add", json!({"def": "levels"})).unwrap()["id"].as_u64().unwrap();
        let sets = |h: &Headless| h.session.history().iter().filter(|l| *l == "Set parameter").count();
        h.command_run("graph.node.param", json!({"id": id, "param": "in_low", "value": [0.3, 0.3, 0.3, 0.3]})).unwrap();
        h.command_run("graph.node.param", json!({"id": id, "param": "in_low", "value": [0.4, 0.4, 0.4, 0.4]})).unwrap();
        assert_eq!(sets(&h), 2, "separate requests must not merge");
    }

    #[test]
    fn same_name_custom_mesh_reload_rebuilds_the_headless_scene() {
        let mut headless = Headless::new(materialcraft_engine::new_session(), Filesystem::Denied);
        let first_mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let second_mesh = b"v 0 0 0\nv 2 0 0\nv 0 1 0\nf 1 2 3\n";
        let options = RenderOptions { width: Some(16), height: Some(16), ..RenderOptions::default() };

        headless.session.load_mesh_bytes("same.obj", first_mesh).unwrap();
        headless.render_3d(&options, 16);
        let first_positions = headless.scene.as_ref().unwrap().1.mesh.positions.clone();
        let previous_mesh = std::sync::Arc::downgrade(&headless.scene.as_ref().unwrap().1.mesh);

        headless.session.load_mesh_bytes("same.obj", second_mesh).unwrap();
        headless.render_3d(&options, 16);

        assert_ne!(headless.scene.as_ref().unwrap().1.mesh.positions, first_positions);
        assert!(previous_mesh.upgrade().is_none(), "the previous headless scene must be released");
    }

    #[test]
    fn full_cache_tracks_document_activation_and_direct_session_undo() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = dir.path().join("doc_a.sbs");
        let path_b = dir.path().join("doc_b.sbs");

        let make_doc = |path: &std::path::Path, color: [f64; 4]| {
            let mut h = Headless::new(materialcraft_engine::new_session(), Filesystem::TrustedLocal);
            let u = h.command_run("graph.node.add", json!({"def": "uniform", "params": {"color": color}})).unwrap()["id"].as_u64().unwrap();
            let o = h.command_run("graph.node.add", json!({"def": "output", "params": {"usage": 0}})).unwrap()["id"].as_u64().unwrap();
            h.command_run("graph.connect", json!({"from": u, "to": o})).unwrap();
            h.save(Some(path.to_str().unwrap())).unwrap();
        };
        make_doc(&path_a, [1.0, 0.0, 0.0, 1.0]);
        make_doc(&path_b, [0.0, 0.0, 1.0, 1.0]);

        let mut h = Headless::new(materialcraft_engine::new_session(), Filesystem::TrustedLocal);
        let opts = RenderOptions { view: Some("2d".into()), width: Some(16), height: Some(16), full: true, ..RenderOptions::default() };

        h.load(path_a.to_str().unwrap()).unwrap();
        let png_a_first = h.render_png(&opts).unwrap().png;

        h.load(path_b.to_str().unwrap()).unwrap();
        let png_b = h.render_png(&opts).unwrap().png;
        assert_ne!(png_a_first, png_b);

        // Re-activating already-open doc A must not reuse doc B's full_cache.
        h.load(path_a.to_str().unwrap()).unwrap();
        let png_a_reactivated = h.render_png(&opts).unwrap().png;
        assert_eq!(png_a_reactivated, png_a_first, "reactivating doc A returned stale full_cache from doc B");

        // Direct mutation via public `session` (undo + inspect clearing dirty) must invalidate full_cache.
        let uniform_id = h.session.graph().unwrap().nodes.iter().find(|n| n.def == "uniform").unwrap().id.0;
        h.command_run("graph.node.param", json!({"id": uniform_id, "param": "color", "value": [0.0, 1.0, 0.0, 1.0]})).unwrap();
        let png_green = h.render_png(&opts).unwrap().png;
        assert_ne!(png_green, png_a_first);

        assert!(h.session.undo().is_some());
        let _ = h.inspect();
        let png_after_direct_undo = h.render_png(&opts).unwrap().png;
        assert_eq!(png_after_direct_undo, png_a_first, "direct session.undo() must invalidate full_cache");
    }

    #[test]
    fn resolve_2d_target_honors_explicit_port_without_node() {
        let mut h = Headless::new(materialcraft_engine::new_session(), Filesystem::Denied);
        h.command_run("file.new", json!({"template": "empty", "name": "Mat"})).unwrap();
        let out = h.command_run("graph.node.add", json!({"def": "output", "params": {"usage": 0}})).unwrap()["id"].as_u64().unwrap();
        let opts = RenderOptions { view: Some("2d".into()), node: None, port: Some(2), ..RenderOptions::default() };
        let (resolved_id, resolved_port) = h.resolve_2d_target(&opts).unwrap();
        assert_eq!(resolved_id.0, out);
        assert_eq!(resolved_port, 2);
    }

    #[test]
    fn render_3d_full_cooks_cross_graph_sources_without_preview_cook() {
        let mut h = Headless::new(materialcraft_engine::new_session(), Filesystem::Denied);
        h.command_run("file.new", json!({"template": "empty", "name": "Main", "size": 64})).unwrap();
        let u0 = h.command_run("graph.node.add", json!({"def": "uniform", "params": {"color": [0.0, 0.0, 0.0, 1.0]}})).unwrap()["id"].as_u64().unwrap();
        let o0 = h.command_run("graph.node.add", json!({"def": "output", "params": {"usage": 0}})).unwrap()["id"].as_u64().unwrap();
        h.command_run("graph.connect", json!({"from": u0, "to": o0})).unwrap();

        let g1_idx = h.command_run("file.new", json!({"template": "empty", "name": "Other", "size": 64})).unwrap()["graph"].as_u64().unwrap();
        let u1 = h.command_run("graph.node.add", json!({"def": "uniform", "params": {"color": [1.0, 0.1, 0.1, 1.0]}})).unwrap()["id"].as_u64().unwrap();
        let o1 = h.command_run("graph.node.add", json!({"def": "output", "params": {"usage": 0}})).unwrap()["id"].as_u64().unwrap();
        h.command_run("graph.connect", json!({"from": u1, "to": o1})).unwrap();

        // Switch back to Main and assign Other's output to baseColor WITHOUT running a preview cook.
        h.command_run("graph.open", json!({"graph": "Main"})).unwrap();
        let opts = RenderOptions { view: Some("3d".into()), width: Some(32), height: Some(32), full: true, ..RenderOptions::default() };
        let dark_png = h.render_png(&opts).unwrap().png;

        h.command_run("view.3d.assign", json!({"graph": g1_idx, "node": o1, "channel": "baseColor"})).unwrap();
        let cross_graph_png = h.render_png(&opts).unwrap().png;
        assert_ne!(cross_graph_png, dark_png, "cross-graph channel must be cooked at full resolution, not silently dropped");
    }

    #[test]
    fn render_png_with_cancel_rejects_preset_cancel_even_on_cache_hit() {
        let mut h = Headless::new(materialcraft_engine::new_session(), Filesystem::Denied);
        h.command_run("file.new", json!({"template": "pbr", "name": "Mat", "size": 32})).unwrap();
        let opts_2d = RenderOptions { view: Some("2d".into()), width: Some(16), height: Some(16), full: true, ..RenderOptions::default() };
        let opts_3d = RenderOptions { view: Some("3d".into()), width: Some(16), height: Some(16), full: true, ..RenderOptions::default() };
        // Populate both caches first.
        h.render_png(&opts_2d).unwrap();
        h.render_png(&opts_3d).unwrap();
        // A pre-set cancel flag must fail before returning cached renders.
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        assert!(h.render_png_with_cancel(&opts_2d, Some(&cancelled)).is_err());
        assert!(h.render_png_with_cancel(&opts_3d, Some(&cancelled)).is_err());
    }
}
