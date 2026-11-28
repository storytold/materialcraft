use craft_texture::{Image, Kind, resource_key};
use materialcraft_core::{Package, Resource};
use materialcraft_formats::{Depth, encode};

use crate::session::Session;

fn normal_map_session() -> Session {
    let mut pkg = Package::new("T");
    let png = encode(&Image::filled(4, 4, Kind::Color, [0.5, 0.5, 1.0, 1.0]), "png", Depth::U8, false).unwrap();
    pkg.resources.push(Resource::new("n", "n.png", materialcraft_core::ResourceBytes::new(png)));
    Session::new(pkg)
}

#[test]
fn the_resource_decoder_reads_data_maps_as_stored() {
    let s = normal_map_session();
    let dec = s.resource_decoder();
    let color = dec(&resource_key("n", false)).unwrap();
    let data = dec(&resource_key("n", true)).unwrap();
    // 8-bit color is sRGB-decoded (0.5 -> ~0.214); a data map keeps 0.5.
    assert!((color.get(0, 0, false)[0] - 0.214).abs() < 0.01);
    assert!((data.get(0, 0, false)[0] - 0.5).abs() < 0.01);
    assert!((data.get(0, 0, false)[2] - 1.0).abs() < 0.01);
}

fn stamp_package() -> (Package, materialcraft_core::NodeId, [materialcraft_core::NodeId; 2]) {
    use materialcraft_core::{Graph, OutputFormat, PatternGraph, PatternNodeKind, ResourceBytes, SizeMode, Value};

    let image = Image::gray(128, 128, |x, y| ((13 * x + 7 * y + 3 * x * y) % 253) as f32 / 255.0);
    let bytes = ResourceBytes::new(encode(&image, "png", Depth::U8, false).unwrap());
    let mut package = Package::new("Stamp sampling");
    for name in ["Field", "Copy"] {
        package.resources.push(Resource::new(name, format!("{name}.png"), bytes.clone()));
    }
    let mut graph = Graph::new("Shared image");
    graph.size = [5; 2];
    graph.format = OutputFormat::F32;
    let input = graph.add_node("input_grayscale", [0.0; 2]);
    let base = &mut graph.node_mut(input).unwrap().base;
    base.size_mode = SizeMode::Absolute;
    base.size = [5; 2];
    base.format = OutputFormat::U8;
    graph.input_images.insert("input".into(), "Field".into());
    graph.input_images_linear.insert("input".into());
    let stamps = [0.0, 80.0].map(|y| {
        let id = graph.add_node("fx_map", [100.0, y]);
        let mut pattern = PatternGraph::with_root(PatternNodeKind::Quadrant);
        pattern.nodes[0].parameters.insert("patterntype".into(), Value::Int(1));
        pattern.nodes[0].parameters.insert("patternsize".into(), Value::Float2([0.5; 2]));
        let node = graph.node_mut(id).unwrap();
        node.base.format = OutputFormat::F32;
        node.params.insert("color_mode".into(), Value::Bool(false));
        node.params.insert("roughness".into(), Value::Float(0.0));
        node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
        graph.connect(input, "output", id, "pattern0").unwrap();
        id
    });
    package.graphs.push(graph);
    (package, input, stamps)
}

#[test]
fn preview_stamps_reuse_sampling_levels_independently() {
    let (package, _, stamps) = stamp_package();
    let mut session = Session::new(package);
    let base = 0x3f01_4181;
    let mip = 0x3f01_8182;
    let pixels = |cook: &materialcraft_eval::Cooked| {
        stamps.map(|id| {
            assert_eq!(cook.nodes[&id].error, None);
            cook.image(id, 0).unwrap().get_gray(9, 8, false).to_bits()
        })
    };
    assert_eq!(pixels(session.cook()), [base; 2]);
    let change_precision = |session: &mut Session| {
        for format in ["u16", "f32"] {
            crate::execute(session, "graph.node.base", &serde_json::json!({"id":stamps[0].0,"format":format})).unwrap();
            session.cook();
        }
    };
    change_precision(&mut session);
    assert_eq!(pixels(session.cook()), [mip, base], "only the edited stamp reacquires its input levels");
    crate::execute(&mut session, "graph.size", &serde_json::json!({"width":32})).unwrap();
    assert_eq!(pixels(session.cook()), [mip, base]);
    assert_eq!(pixels(&session.cook_full(0)), [base; 2]);
    for extent in [64, 32] {
        crate::execute(&mut session, "graph.size", &serde_json::json!({"width":extent})).unwrap();
    }
    assert_eq!(pixels(session.cook()), [base; 2]);
    change_precision(&mut session);
    assert_eq!(pixels(session.cook()), [mip, base]);
    crate::execute(&mut session, "graph.input.bind", &serde_json::json!({"identifier":"input","resource":"Copy","linear":true})).unwrap();
    assert_eq!(pixels(session.cook()), [base; 2]);
}

#[test]
fn changing_one_instances_precision_updates_its_fixed_precision_stamp() {
    use materialcraft_core::{Graph, OutputFormat, Value};

    let (mut package, source, stamps) = stamp_package();
    let child = &mut package.graphs[0];
    let out = child.add_node("output", [200.0, 0.0]);
    child.connect(stamps[0], "output", out, "input").unwrap();
    let mut parent = Graph::new("Paired instances");
    parent.size = [5; 2];
    parent.format = OutputFormat::F32;
    parent.input_images = std::mem::take(&mut child.input_images);
    parent.input_images_linear = std::mem::take(&mut child.input_images_linear);
    let input = parent.add_node("input_grayscale", [0.0; 2]);
    parent.node_mut(input).unwrap().base = child.node(source).unwrap().base.clone();
    let instances = [0.0, 80.0].map(|y| {
        let instance = parent.add_node("graph_instance", [100.0, y]);
        let node = parent.node_mut(instance).unwrap();
        node.params.insert("graph".into(), Value::Text(child.name.clone()));
        node.base.format = OutputFormat::F32;
        parent.connect(input, "output", instance, "input").unwrap();
        instance
    });
    package.graphs.insert(0, parent);
    let mut session = Session::new(package);
    let pixels = |cooked: &materialcraft_eval::Cooked| {
        instances.map(|id| {
            assert_eq!(cooked.nodes[&id].error, None);
            cooked.image(id, 0).unwrap().get_gray(9, 8, false).to_bits()
        })
    };
    assert_eq!(pixels(session.cook()), [0x3f01_4181; 2]);
    for format in ["u16", "f32"] {
        crate::execute(&mut session, "graph.node.base", &serde_json::json!({"id":instances[0].0,"format":format})).unwrap();
        assert_eq!(pixels(session.cook()), [0x3f01_8182, 0x3f01_4181]);
    }
    assert_eq!(pixels(&session.cook_full(0)), [0x3f01_4181; 2]);
}

#[test]
fn bitmap_nodes_read_default_colors_from_the_file_and_keep_explicit_data_reads() {
    use materialcraft_core::{Graph, OutputFormat, ResourceBytes, Value};

    let field = Image::filled(16, 16, Kind::Color, [64.0, 128.0, 192.0, 97.0].map(|v| v / 255.0));
    let png = encode(&field, "png", Depth::U8, false).unwrap();
    for (gamma_chunk, expected) in [(false, [64, 128, 192, 97]), (true, [136, 186, 224, 97])] {
        let mut bytes = png.clone();
        if gamma_chunk {
            // Insert a unit gAMA chunk, including its CRC, immediately after IHDR.
            bytes.splice(33..33, *b"\0\0\0\x04gAMA\0\x01\x86\xa0\x31\xe8\x96\x5f");
        }
        let mut package = Package::new("Bitmap file colors");
        package.resources.push(Resource::new("field", "field.png", ResourceBytes::new(bytes)));
        let mut graph = Graph::new("File and data reads");
        graph.size = [4, 4];
        graph.format = OutputFormat::U8;
        let bitmap = graph.add_node("bitmap", [0.0; 2]);
        let data = graph.add_node("bitmap", [0.0, 80.0]);
        let input = graph.add_node("input_color", [0.0, 160.0]);
        graph.input_images.insert("input".into(), "field".into());
        graph.input_images_from_file.insert("input".into());
        for id in [bitmap, data] {
            graph.node_mut(id).unwrap().params.insert("resource".into(), Value::Text("field".into()));
        }
        graph.node_mut(data).unwrap().params.insert("colour_space".into(), Value::Enum(1));
        package.graphs.push(graph);
        let mut session = Session::new(package);
        let cook = session.cook();
        let pixel = |id| cook.image(id, 0).unwrap().get(0, 0, false).map(|v| (v * 255.0).round() as u8);
        assert_eq!(pixel(bitmap), expected);
        assert_eq!(pixel(data), [64, 128, 192, 97]);
        assert_eq!(pixel(input), expected);
        for (linear, levels) in [(Some(true), [64, 128, 192, 97]), (None, expected), (Some(false), [13, 55, 134, 97]), (None, expected)] {
            let mut params = serde_json::json!({"identifier": "input", "resource": "field"});
            if let Some(value) = linear {
                params["linear"] = value.into();
            }
            crate::execute(&mut session, "graph.input.bind", &params).unwrap();
            assert_eq!(session.graph().unwrap().input_images_from_file.contains("input"), linear.is_none());
            let rebound = session.cook().image(input, 0).unwrap().get(0, 0, false).map(|v| (v * 255.0).round() as u8);
            assert_eq!(rebound, levels);
        }
    }
}

#[cfg(test)]
mod merge_tests {
    use crate::Session;

    fn set(s: &mut Session, key: &str) {
        s.edit_merged(key, "Set parameter", |s| {
            s.modified = true;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn same_key_merges_until_finish_edit() {
        let mut s = crate::new_session();
        let base = s.history().len();
        set(&mut s, "k");
        set(&mut s, "k");
        assert_eq!(s.history().len(), base + 1, "one drag is one entry");
        s.finish_edit();
        set(&mut s, "k");
        assert_eq!(s.history().len(), base + 2, "a second drag after finish_edit is a new entry");
    }
}

#[test]
fn failed_merged_edits_restore_clean_flags_and_history() {
    let mut s = crate::sample_session();
    s.edit_merged("k", "Set", |_| Ok(())).unwrap();
    s.modified = false;
    s.dirty = false;
    let name = s.pkg.name.clone();
    let history = s.history();
    let result: crate::Result<()> = s.edit_merged("k", "Set", |s| {
        s.pkg.name = "partial".into();
        s.changed();
        Err(crate::EngineError::Other("failed".into()))
    });
    assert!(result.is_err());
    assert_eq!(s.pkg.name, name);
    assert_eq!(s.history(), history);
    assert!(!s.modified && !s.dirty);
}

#[cfg(test)]
mod group_tests {
    use crate::{EngineError, Session, execute};
    use serde_json::json;

    fn two_nodes() -> (Session, u64, u64) {
        let mut s = crate::sample_session();
        execute(&mut s, "file.new", &json!({"template": "empty"})).unwrap();
        let a = execute(&mut s, "graph.node.add", &json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap();
        let b = execute(&mut s, "graph.node.add", &json!({"def": "levels"})).unwrap()["id"].as_u64().unwrap();
        (s, a, b)
    }

    #[test]
    fn a_group_is_one_undo_entry_and_one_undo_reverts_all() {
        let (mut s, a, b) = two_nodes();
        let base = s.history().len();
        s.group("Rewire", |s| {
            execute(s, "graph.connect", &json!({"from": a, "to": b, "toPort": "input"}))?;
            execute(s, "graph.disconnect", &json!({"to": b, "toPort": "input"}))?;
            execute(s, "graph.connect", &json!({"from": a, "to": b, "toPort": "input"}))?;
            Ok(())
        })
        .unwrap();
        assert_eq!(s.history().len(), base + 1);
        assert_eq!(s.history().last().map(String::as_str), Some("Rewire"));
        assert_eq!(s.undo().as_deref(), Some("Rewire"));
        assert!(s.graph().unwrap().connections.is_empty(), "the single undo removed every step");
    }

    #[test]
    fn a_failing_group_restores_everything_and_records_nothing() {
        let (mut s, a, b) = two_nodes();
        let base = s.history().len();
        let r: Result<(), EngineError> = s.group("Rewire", |s| {
            execute(s, "graph.connect", &json!({"from": a, "to": b, "toPort": "input"}))?;
            Err(EngineError::Other("boom".into()))
        });
        assert!(r.is_err());
        assert_eq!(s.history().len(), base);
        assert!(s.graph().unwrap().connections.is_empty());
    }

    #[test]
    fn begin_and_end_group_span_several_calls_and_an_untouched_group_records_nothing() {
        let (mut s, a, b) = two_nodes();
        let base = s.history().len();
        s.begin_group("Nothing");
        s.end_group();
        assert_eq!(s.history().len(), base);
        s.begin_group("Both");
        execute(&mut s, "graph.connect", &json!({"from": a, "to": b, "toPort": "input"})).unwrap();
        execute(&mut s, "graph.node.param", &json!({"id": b, "param": "in_low", "value": [0.2, 0.2, 0.2, 0.2]})).unwrap();
        s.end_group();
        assert_eq!(s.history().len(), base + 1);
    }
}

#[test]
fn a_group_with_only_failed_edits_preserves_redo_and_clean_flags() {
    let mut s = crate::sample_session();
    s.edit("Prior", |s| {
        s.pkg.name = "Prior".into();
        Ok(())
    })
    .unwrap();
    s.undo();
    assert!(s.can_redo());
    s.modified = false;
    s.dirty = false;
    let history = s.history();
    let name = s.pkg.name.clone();
    s.begin_group("Rejected");
    let result: crate::Result<()> = s.edit("Bad", |s| {
        s.pkg.name = "Partial".into();
        Err(crate::EngineError::Other("failed".into()))
    });
    assert!(result.is_err());
    s.end_group();
    assert_eq!(s.pkg.name, name);
    assert_eq!(s.history(), history);
    assert!(s.can_redo());
    assert!(!s.modified && !s.dirty);
}

#[test]
fn a_caught_nested_group_error_rolls_back_only_its_own_edits() {
    let mut s = crate::sample_session();
    let original = s.pkg.name.clone();
    let count = s.history().len();
    s.group("Outer", |s| {
        s.edit("Outer edit", |s| {
            s.pkg.name = "Outer".into();
            Ok(())
        })?;
        let result: crate::Result<()> = s.group("Inner", |s| {
            s.edit("Inner edit", |s| {
                s.pkg.name = "Partial inner".into();
                Ok(())
            })?;
            Err(crate::EngineError::Other("failed".into()))
        });
        assert!(result.is_err());
        assert_eq!(s.pkg.name, "Outer");
        Ok(())
    })
    .unwrap();
    assert_eq!(s.history().len(), count + 1);
    s.undo();
    assert_eq!(s.pkg.name, original);
}

#[test]
fn a_failed_group_recooks_if_it_cooked_a_temporary_document() {
    use crate::{core::NodeId, execute};
    use serde_json::json;
    let mut s = crate::new_session();
    s.preview_size = Some(16);
    let id = NodeId(execute(&mut s, "graph.node.add", &json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap());
    execute(&mut s, "graph.node.param", &json!({"id": id.0, "param": "color_mode", "value": true})).unwrap();
    let before = s.cook().image(id, 0).unwrap().data().to_vec();
    let result: crate::Result<()> = s.group("Temporary", |s| {
        execute(s, "graph.node.param", &json!({"id": id.0, "param": "color", "value": [0.1, 0.8, 0.2, 1.0]}))?;
        s.cook();
        Err(crate::EngineError::Other("failed".into()))
    });

    assert!(result.is_err());

    s.cook_if_dirty();

    assert_eq!(s.cooked.image(id, 0).unwrap().data(), before.as_slice());
}
