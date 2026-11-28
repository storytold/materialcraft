use super::embedded::{base64_engine, decode_embedded, encode_embedded};
use super::read::{is_sbs, read_sbs};
use super::write::write_sbs;
use base64::Engine as _;
use materialcraft_core::{Frame, Graph, MAX_SIZE_LOG2, MIN_SIZE_LOG2, Package, Resource, ResourceBytes, SizeMode, Value, missing_ports};

fn no_linked_files(_: &str) -> Option<Vec<u8>> {
    None
}

#[test]
fn connectable_function_argument_survives_save_and_reopen() {
    let xml = br#"<package><content><function><identifier v="ReadAmount"/><uid v="1"/><type v="256"/>
        <paraminputs><paraminput><identifier v="amount"/><uid v="2"/><isConnectable v="1"/>
        <type v="256"/><defaultValue><constantValueFloat1 v=".1875"/></defaultValue></paraminput></paraminputs>
        <paramValue><dynamicValue><rootnode v="3"/><paramNodes><paramNode><uid v="3"/><function v="get_float1"/>
        <type v="256"/><funcDatas><funcData><name v="get"/><constantValue><constantValueString v="amount"/>
        </constantValue></funcData></funcDatas></paramNode></paramNodes></dynamicValue></paramValue></function></content></package>"#;
    let package = read_sbs(xml, "argument.sbs", &no_linked_files).unwrap().pkg;
    let graph = &package.graphs[0];
    assert_eq!(graph.input_value("amount"), Some(&Value::Float(0.1875)));
    let saved = write_sbs(&package).unwrap();
    let document = roxmltree::Document::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
    let argument = document.descendants().find(|node| node.has_tag_name("paraminput")).unwrap();
    assert_eq!(super::xml::child_value(argument, "isConnectable"), Some("1"));
    let reopened = read_sbs(&saved, "argument.sbs", &no_linked_files).unwrap().pkg;
    assert_eq!(reopened.graphs[0].exposed, graph.exposed);
}

#[test]
fn absolute_graph_output_size_survives_parent_size_changes_and_save() {
    let xml = br#"<package><content><graph><identifier v="Fixed"/>
        <baseParameters><parameter><name v="outputsize"/><relativeTo v="0"/>
        <paramValue><constantValueInt2 v="6 5"/></paramValue></parameter></baseParameters>
        <options><option><name v="defaultParentSize"/><value v="7x8"/></option></options>
        </graph></content></package>"#;
    let mut package = read_sbs(xml, "fixed.sbs", &no_linked_files).unwrap().pkg;
    assert_eq!(package.graphs[0].size, [7, 8]);
    assert_eq!(package.graphs[0].size_px(), [64, 32]);
    package.graphs[0].size = [8, 7];
    let saved = write_sbs(&package).unwrap();
    let loaded = read_sbs(&saved, "fixed.sbs", &no_linked_files).unwrap().pkg;
    assert_eq!(loaded.graphs[0].size, [8, 7]);
    assert_eq!(loaded.graphs[0].size_px(), [64, 32]);
}

#[test]
fn small_default_parent_sizes_round_trip_per_axis() {
    for (width, height) in [(0, 0), (0, 3), (2, 0), (1, 1), (2, 3), (3, 3), (4, 2), (5, 4)] {
        let xml = format!(
            r#"<package><content><graph><identifier v="Small"/>
            <options><option><name v="defaultParentSize"/><value v="{width}x{height}"/></option></options>
            </graph></content></package>"#
        );
        let package = read_sbs(xml.as_bytes(), "small.sbs", &no_linked_files).unwrap().pkg;
        assert_eq!(package.graphs[0].size, [width, height]);
        let saved = write_sbs(&package).unwrap();
        let loaded = read_sbs(&saved, "small.sbs", &no_linked_files).unwrap().pkg;
        assert_eq!(loaded.graphs[0].size_px(), [1 << width, 1 << height]);
    }
}

#[test]
fn shuffle_roundtrip_preserves_signed_channel_selectors() {
    for selector in [-2_i32, -1, 8, 9, 16, 255] {
        let xml = format!(
            r#"<package><content><graph><identifier v="Original"/><compNodes>
          <compNode><uid v="1"/><compImplementation><compFilter><filter v="shuffle"/><parameters>
            <parameter><name v="channelred"/><paramValue><constantValueInt32 v="{selector}"/></paramValue></parameter>
          </parameters></compFilter></compImplementation></compNode>
        </compNodes></graph></content></package>"#
        );

        let loaded = read_sbs(xml.as_bytes(), "original.sbs", &no_linked_files).unwrap();
        let node = &loaded.pkg.graphs[0].nodes[0];

        assert_eq!(node.params.get("red"), Some(&Value::Enum(selector as u32)), "selector {selector}");

        let saved = write_sbs(&loaded.pkg).unwrap();
        let reloaded = read_sbs(&saved, "original.sbs", &no_linked_files).unwrap();

        assert_eq!(reloaded.pkg.graphs[0].nodes[0].params.get("red"), node.params.get("red"), "saved selector {selector}");
    }
}

#[test]
fn saved_dot_outputs_load_as_incomplete_functions() {
    let xml = r#"<package><identifier v="Dots"/><content><function><identifier v="Incomplete"/>
      <paramValue><dynamicValue><rootnode v="2"/><paramNodes>
        <paramNode><uid v="1"/><function v="const_float1"/></paramNode>
        <paramNode><uid v="2"/><function v="passthrough"/><connections>
          <connection><identifier v="input"/><connRef v="1"/></connection>
        </connections></paramNode>
      </paramNodes></dynamicValue></paramValue></function></content></package>"#;
    let loaded = read_sbs(xml.as_bytes(), "dots.sbs", &no_linked_files).unwrap();
    let function = loaded.pkg.graphs.iter().find_map(|graph| graph.function.as_ref()).unwrap();
    assert_eq!(function.output, None);
    assert_eq!(function.nodes.len(), 2);
    assert_eq!(function.nodes[1].inputs, [Some(function.nodes[0].id)]);
}

fn sample_package() -> Package {
    let mut pkg = Package::new("Primitives");
    let mut graph = Graph::new("Wall");
    graph.size = [9, 9];

    let curve = graph.add_node("curve", [0.0, 0.0]);
    let blur = graph.add_node("blur", [150.0, 0.0]);
    let uniform = graph.add_node("uniform", [0.0, 150.0]);
    let blend = graph.add_node("blend", [300.0, 0.0]);
    let output = graph.add_node("output", [450.0, 0.0]);
    graph.connect(curve, "output", blur, "input").unwrap();
    graph.connect(blur, "output", blend, "foreground").unwrap();
    graph.connect(uniform, "output", blend, "background").unwrap();
    graph.connect(blend, "output", output, "input").unwrap();

    let blur_node = graph.node_mut(blur).unwrap();
    blur_node.params.insert("intensity".into(), Value::Float(4.5));
    blur_node.base.size_mode = SizeMode::Absolute;
    blur_node.base.size = [8, 8];
    graph.node_mut(uniform).unwrap().params.insert("color".into(), Value::Color([0.25, 0.5, 0.75, 1.0]));
    graph.node_mut(blend).unwrap().params.insert("mode".into(), Value::Enum(3));
    let output_node = graph.node_mut(output).unwrap();
    output_node.params.insert("usage".into(), Value::Enum(4));
    output_node.params.insert("identifier".into(), Value::Text("height".into()));
    graph.node_mut(curve).unwrap().params.insert("curve".into(), Value::Curve(materialcraft_core::Curve::default()));
    graph.frames.push(Frame {
        id: 90,
        title: "Frame & <title>".into(),
        title_visible: false,
        description: "note".into(),
        rect: [-10.0, -10.0, 300.0, 200.0],
        color: [0.1, 0.2, 0.3, 0.4],
    });
    pkg.graphs.push(graph);

    let orange = craft_texture::Image::filled(4, 4, craft_texture::Kind::Color, [1.0, 0.5, 0.0, 1.0]);
    let png = crate::encode(&orange, "png", crate::Depth::U8, true).unwrap();
    pkg.resources.push(Resource::new("tex", "tex.png", ResourceBytes::new(png)));
    pkg
}

#[test]
fn sniffs_sbs() {
    assert!(is_sbs(b"<?xml version=\"1.0\"?>\n<package><content/></package>"));
    assert!(is_sbs(b"\xef\xbb\xbf<package>"));
    assert!(is_sbs(b"<!-- hi --><package >"));
    assert!(!is_sbs(b"<packages>"));
    assert!(!is_sbs(b"PK\x03\x04zip"));
    assert!(!is_sbs(b"<?xml"));
    assert!(!is_sbs(b""));
}

#[test]
fn first_sbs_save_declares_the_graph_schema_and_current_updater() {
    let unversioned = read_sbs(b"<package><content/></package>", "unversioned.sbs", &no_linked_files).unwrap().pkg;
    for package in [Package::new("New file"), unversioned] {
        assert!(package.format_version.is_none());

        let text = String::from_utf8(write_sbs(&package).unwrap()).unwrap();
        let document = roxmltree::Document::parse(&text).unwrap();
        assert_eq!(super::xml::child_value(document.root_element(), "formatVersion"), Some("1.1.0.202401"));
        assert_eq!(super::xml::child_value(document.root_element(), "updaterVersion"), Some(env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn later_sbs_saves_upgrade_release_headers_and_keep_existing_schema_revisions() {
    for (baseline, saved) in [("0.0.7", "1.1.0.202401"), ("0.1.3", "1.1.0.202401"), ("1.1.0.202401", "1.1.0.202401"), ("1.1.0.202502", "1.1.0.202502")] {
        let old = format!(r#"<package><identifier v="Older file"/><formatVersion v="{baseline}"/><updaterVersion v="0.0.1"/><content/></package>"#);
        let mut package = read_sbs(old.as_bytes(), "older.sbs", &no_linked_files).unwrap().pkg;
        assert_eq!(package.format_version.as_deref(), Some(baseline));
        for _ in 0..3 {
            // Snapshot/restore and subsequent saves must retain the baseline too
            package = serde_json::from_slice(&serde_json::to_vec(&package).unwrap()).unwrap();
            let bytes = write_sbs(&package).unwrap();
            let document = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();

            assert_eq!(super::xml::child_value(document.root_element(), "formatVersion"), Some(saved));
            assert_eq!(super::xml::child_value(document.root_element(), "updaterVersion"), Some(env!("CARGO_PKG_VERSION")));

            package = read_sbs(&bytes, "saved.sbs", &no_linked_files).unwrap().pkg;
            assert_eq!(package.format_version.as_deref(), Some(saved));
        }
    }
}

#[test]
fn graph_inputs_write_one_lane_integer_defaults() {
    let source = br#"<package><content><graph><identifier v="settings"/><paraminputs>
        <paraminput><identifier v="count"/><uid v="17"/><type v="16"/><defaultValue><constantValueInt32 v="-3"/></defaultValue></paraminput>
        <paraminput><identifier v="mode"/><uid v="18"/><type v="16"/><defaultValue><constantValueInt1 v="37"/></defaultValue></paraminput>
        </paraminputs></graph></content></package>"#;
    let mut pkg = read_sbs(source, "settings.sbs", &no_linked_files).unwrap().pkg;
    let graph = &mut pkg.graphs[0];
    let mode = graph.exposed.iter_mut().find(|parameter| parameter.id == "mode").unwrap();
    mode.default = Value::Enum(37);
    let input = graph.add_node("input_value", [0.0, 0.0]);
    let node = graph.node_mut(input).unwrap();
    node.params.insert("identifier".into(), Value::Text("wire".into()));
    node.params.insert("default".into(), Value::Int(16_777_217));
    let bytes = write_sbs(&pkg).unwrap();
    let doc = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    let defaults: Vec<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("paraminput"))
        .map(|input| {
            let default = super::xml::child_element(input, "defaultValue").unwrap().children().find(roxmltree::Node::is_element).unwrap();
            (super::xml::child_value(input, "identifier").unwrap(), default.tag_name().name(), default.attribute("v").unwrap())
        })
        .collect();
    assert_eq!(defaults, [("wire", "constantValueInt1", "16777217"), ("count", "constantValueInt1", "-3"), ("mode", "constantValueInt1", "37")]);
}

#[test]
fn structural_round_trip() {
    let pkg = sample_package();
    let plain = String::from_utf8(write_sbs(&pkg).unwrap()).unwrap();
    assert!(!plain.contains("materialcraft-package"));
    assert!(!plain.contains("materialcraft://"));
    let read = read_sbs(plain.as_bytes(), "x.sbs", &no_linked_files).unwrap();

    let (original, back) = (&pkg.graphs[0], &read.pkg.graphs[0]);
    assert_eq!(back.name, "Wall");
    assert_eq!(back.size, [9, 9]);
    assert_eq!(back.nodes.len(), original.nodes.len());
    assert_eq!(back.connections.len(), original.connections.len());
    for (a, b) in original.nodes.iter().zip(&back.nodes) {
        assert_eq!(a.def, b.def);
        assert!((a.pos[0] - b.pos[0]).abs() < 1e-3 && (a.pos[1] - b.pos[1]).abs() < 1e-3);
        assert_eq!(a.base, b.base);
    }

    assert_eq!(back.nodes[1].params.get("intensity"), Some(&Value::Float(4.5)));
    let Some(Value::Color(color)) = back.nodes[2].params.get("color") else { panic!("uniform color") };

    assert!((color[0] - 0.25).abs() < 1e-3 && (color[2] - 0.75).abs() < 1e-3);
    assert_eq!(back.nodes[3].params.get("mode"), Some(&Value::Enum(3)));
    assert_eq!(back.nodes[4].params.get("usage"), Some(&Value::Enum(4)));
    assert_eq!(back.nodes[4].params.get("identifier"), Some(&Value::Text("height".into())));
    assert_eq!(back.frames[0].title, "Frame & <title>");
    assert!(!back.frames[0].title_visible);
    assert_eq!(read.pkg.resources[0].bytes, pkg.resources[0].bytes);

    let blend = back.nodes[3].id;
    assert_eq!(back.input_of(blend, "foreground").map(|c| c.from), Some(back.nodes[1].id));
    assert_eq!(back.input_of(blend, "background").map(|c| c.from), Some(back.nodes[2].id));
}

#[test]
fn native_data_is_the_saved_document() {
    let text = String::from_utf8(write_sbs(&sample_package()).unwrap()).unwrap().replace("<identifier v=\"Wall\"/>", "<identifier v=\"Wall2\"/>");
    let read = read_sbs(text.as_bytes(), "x.sbs", &no_linked_files).unwrap();

    assert_eq!(read.pkg.graphs[0].name, "Wall2");
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
}

#[test]
fn external_references_and_unknown_filters_survive_saving() {
    let xml = r#"<?xml version="1.0"?><package><identifier v="P"/><dependencies><dependency><filename v="sbs://noise_fancy.sbs"/><uid v="7"/></dependency></dependencies><content><graph><identifier v="G"/>
        <compNodes>
          <compNode><uid v="1"/><GUILayout><gpos v="0 0 0"/></GUILayout><compOutputs><compOutput><uid v="11"/><comptype v="2"/></compOutput></compOutputs>
            <compImplementation><compInstance><path v="pkg:///fancy_noise?dependency=7"/><parameters><parameter><name v="scale"/><paramValue><constantValueFloat1 v="3"/></paramValue></parameter><parameter><name v="detail"/><paramValue><dynamicValue><rootnode v="77"/><paramNodes><paramNode><uid v="77"/><type v="256"/><function v="const_float1"/></paramNode></paramNodes></dynamicValue></paramValue></parameter></parameters>
            <outputBridgings><outputBridging><uid v="11"/><identifier v="noise"/></outputBridging></outputBridgings></compInstance></compImplementation></compNode>
          <compNode><uid v="2"/><GUILayout><gpos v="200 0 0"/></GUILayout><compOutputs><compOutput><uid v="21"/><comptype v="1"/></compOutput></compOutputs>
            <connections><connection><identifier v="input1"/><connRef v="1"/><connRefOutput v="11"/></connection></connections>
            <compImplementation><compFilter><filter v="blur"/><parameters><parameter><name v="intensity"/><paramValue><constantValueFloat1 v="6"/></paramValue></parameter><parameter><name v="bogus"/><paramValue><constantValueFloat1 v="6"/></paramValue></parameter></parameters></compFilter></compImplementation></compNode>
          <compNode><uid v="3"/><GUILayout><gpos v="400 0 0"/></GUILayout><compOutputs><compOutput><uid v="31"/><comptype v="1"/></compOutput></compOutputs>
            <connections><connection><identifier v="weird_in"/><connRef v="2"/><connRefOutput v="21"/></connection></connections>
            <compImplementation><compFilter><filter v="futurefilter"/><parameters/></compFilter></compImplementation></compNode>
        </compNodes></graph></content></package>"#;
    let read = read_sbs(xml.as_bytes(), "p.sbs", &no_linked_files).unwrap();
    let graph = &read.pkg.graphs[0];
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.connections.len(), 2);

    let fancy = &graph.nodes[0];
    assert_eq!(fancy.def, "graph_instance");
    let instance = fancy.instance.as_ref().unwrap();
    assert_eq!(instance.package, "sbs://noise_fancy.sbs");
    assert_eq!(instance.graph, "fancy_noise");
    assert_eq!(instance.outputs[0].identifier, "noise");
    assert_eq!(instance.outputs[0].type_code, 2);
    assert_eq!(fancy.params.get("scale"), Some(&Value::Float(3.0)));
    assert_eq!(graph.nodes[1].params.get("intensity"), Some(&Value::Float(6.0)));
    assert_eq!(graph.input_of(graph.nodes[1].id, "input").map(|c| c.from_port.as_str()), Some("noise"));
    assert_eq!(missing_ports(&graph.nodes[2]).0, vec!["weird_in".to_string()]);
    for expected in ["missing node `futurefilter`", "bogus"] {
        assert!(read.warnings.iter().any(|w| w.contains(expected)), "{expected}: {:?}", read.warnings);
    }

    // References and unavailable operations survive without embedding their implementations
    let text = String::from_utf8(write_sbs(&read.pkg).unwrap()).unwrap();
    assert!(text.contains("sbs://noise_fancy.sbs"));
    assert!(text.contains("pkg:///fancy_noise?dependency="));
    assert!(text.contains("<filter v=\"futurefilter\"/>"));
    let back = read_sbs(text.as_bytes(), "p.sbs", &no_linked_files).unwrap();
    let back_graph = &back.pkg.graphs[0];
    assert_eq!(back_graph.nodes[0].def, "graph_instance");
    assert_eq!(back_graph.nodes[0].instance, fancy.instance);
    assert_eq!(back_graph.nodes[0].params, fancy.params);
    assert_eq!(back_graph.nodes[0].functions["detail"].nodes.len(), 1);
    assert_eq!(back_graph.nodes[0].functions["detail"].nodes[0].value, fancy.functions["detail"].nodes[0].value);
    assert_eq!(back_graph.connections.len(), 2);
}

#[test]
fn wires_keep_the_last_per_input_and_drop_cycles() {
    let node = |uid: u32, wires: &str| {
        format!(
            "<compNode><uid v=\"{uid}\"/><compOutputs><compOutput><uid v=\"{}\"/></compOutput></compOutputs><connections>{wires}</connections><compImplementation><compFilter><filter v=\"blend\"/></compFilter></compImplementation></compNode>",
            uid + 100
        )
    };

    let wire =
        |input: &str, from: u32| format!("<connection><identifier v=\"{input}\"/><connRef v=\"{from}\"/><connRefOutput v=\"{}\"/></connection>", from + 100);
    let xml = format!(
        "<package><content><graph><identifier v=\"G\"/><compNodes>{}{}{}{}</compNodes></graph></content></package>",
        node(1, ""),
        node(2, &format!("{}{}", wire("source", 1), wire("source", 4))),
        node(3, &wire("source", 2)),
        node(4, &wire("source", 3))
    );
    let read = read_sbs(xml.as_bytes(), "x.sbs", &no_linked_files).unwrap();
    let graph = &read.pkg.graphs[0];

    // 2 takes 4 (the last wire), which closes 2 → 3 → 4 → 2, so one wire of that loop is dropped
    assert_eq!(graph.connections.len(), 2);
    assert!(graph.input_of(graph.nodes[1].id, "foreground").is_none_or(|c| c.from == graph.nodes[3].id));
    assert_eq!(graph.topo_order().len(), 4);
    assert!(read.warnings.iter().any(|w| w.contains("cycle")), "{:?}", read.warnings);
}

#[test]
fn hostile_input_is_an_error_or_a_warning_never_a_panic() {
    let cases: &[&str] = &[
        "",
        "<package",
        "<notpackage/>",
        "<!DOCTYPE package [<!ENTITY x \"y\">]><package/>",
        "<package><content><graph/></content></package>",
        "<package><content><graph><compNodes><compNode/><compNode><uid v=\"1\"/><compImplementation><compInstance/></compImplementation></compNode><compNode><uid v=\"1\"/></compNode></compNodes></graph></content></package>",
        "<package><content><graph><options><option><name v=\"defaultParentSize\"/><value v=\"99999x-4\"/></option></options><compNodes><compNode><uid v=\"1\"/><connections><connection><identifier v=\"input1\"/><connRef v=\"1\"/><connRefOutput v=\"9\"/></connection></connections><compImplementation><compFilter><filter v=\"blur\"/><parameters><parameter><name v=\"outputsize\"/><relativeTo v=\"0\"/><paramValue><constantValueInt2 v=\"99999999999 nan\"/></paramValue></parameter><parameter><name v=\"intensity\"/><paramValue><constantValueFloat1 v=\"inf\"/></paramValue></parameter></parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>",
        "<package><content><resource><identifier v=\"r\"/><source><binembedded><strdata v=\"////\"/></binembedded></source></resource><resource><identifier v=\"s\"/><source><binembedded><strdata v=\"f////w==\"/></binembedded></source></resource><resource><identifier v=\"t\"/><filepath v=\"../../etc/passwd\"/></resource></content></package>",
        "<package><content/><!-- materialcraft-package version=1 hash=0 data=AAAA --></package>",
        "<package><content><group><content><group><content><graph><identifier v=\"Deep\"/></graph></content></group></content></group><function><identifier v=\"f\"/></function><weird/></content></package>",
    ];
    for case in cases {
        if let Ok(read) = read_sbs(case.as_bytes(), "x.sbs", &no_linked_files) {
            assert!(write_sbs(&read.pkg).is_ok(), "{case}");
        }
    }
    assert!(read_sbs(b"\xff\xfe", "x.sbs", &no_linked_files).is_err());

    let nested = read_sbs(cases[9].as_bytes(), "x.sbs", &no_linked_files).unwrap();
    assert!(nested.pkg.graphs.iter().any(|graph| graph.name == "Deep"));
    assert!(nested.pkg.graphs.iter().any(|graph| graph.name == "f" && graph.function.is_some()));
    assert!(nested.warnings.iter().any(|warning| warning.contains("missing function body")));
    let clamped = read_sbs(cases[6].as_bytes(), "x.sbs", &no_linked_files).unwrap();
    assert_eq!(clamped.pkg.graphs[0].size, [MAX_SIZE_LOG2, MIN_SIZE_LOG2]);
    assert!(clamped.pkg.graphs[0].connections.is_empty(), "a self-wire is a cycle");
}

#[test]
fn linked_resources_load_through_the_callback() {
    let xml = "<package><content><resource><identifier v=\"img\"/><type v=\"bitmap\"/><format v=\"png\"/><source><externalcopy><filename v=\"p.resources/img.png\"/></externalcopy></source></resource></content></package>";

    let found = read_sbs(xml.as_bytes(), "p.sbs", &|path| (path == "p.resources/img.png").then(|| vec![1, 2, 3])).unwrap();
    assert_eq!(found.pkg.resources[0].bytes.as_slice(), &[1, 2, 3]);

    let not_found = read_sbs(xml.as_bytes(), "p.sbs", &no_linked_files).unwrap();
    assert!(not_found.pkg.resources[0].bytes.is_empty());
    assert_eq!(not_found.pkg.resources[0].source, "p.resources/img.png");
    assert!(not_found.warnings.iter().any(|w| w.contains("img")), "{:?}", not_found.warnings);
}

#[test]
fn duplicate_resource_identifiers_get_unique_names() {
    let resources: String = (0..2_000)
        .map(|_| "<resource><identifier v=\"img\"/><type v=\"bitmap\"/><format v=\"png\"/><source><binembedded><strdata v=\"AAAA\"/></binembedded></source></resource>")
        .collect();
    let xml = format!("<package><content>{resources}</content></package>");
    let read = read_sbs(xml.as_bytes(), "many.sbs", &no_linked_files).unwrap();
    let names: std::collections::HashSet<&str> = read.pkg.resources.iter().map(|resource| resource.name.as_str()).collect();
    assert_eq!(names.len(), read.pkg.resources.len(), "resource names stay unique");
    assert!(read.pkg.resources[0].name == "img");
    assert!(read.pkg.resources.iter().skip(1).all(|resource| resource.name.starts_with("img_")));
}

#[test]
fn embedded_data_round_trips_and_checks_its_length() {
    let data: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
    let encoded = encode_embedded(&data).unwrap();

    assert_eq!(decode_embedded(&encoded).unwrap(), data);

    let mut raw = base64_engine().decode(&encoded).unwrap();
    raw[3] ^= 1;

    assert!(decode_embedded(&base64_engine().encode(raw)).unwrap_err().contains("declared"));
}

#[test]
fn native_function_fixture_retains_operation_types_and_ports() {
    let xml = r#"<package><content><graph><identifier v="Values"/><compNodes><compNode><uid v="10"/>
      <compOutputs><compOutput><uid v="11"/><comptype v="16"/></compOutput></compOutputs>
      <compImplementation><compFilter><filter v="valueprocessor"/><parameters><parameter><name v="function"/><paramValue><dynamicValue>
        <rootnode v="22"/><paramNodes>
          <paramNode><uid v="20"/><function v="const_int1"/><type v="16"/><funcDatas><funcData><name v="const_int1"/><constantValue><constantValueInt32 v="16777217"/></constantValue></funcData></funcDatas></paramNode>
          <paramNode><uid v="21"/><function v="const_int1"/><type v="16"/><funcDatas><funcData><name v="const_int1"/><constantValue><constantValueInt32 v="3"/></constantValue></funcData></funcDatas></paramNode>
          <paramNode><uid v="22"/><function v="add"/><type v="16"/><connections><connection><identifier v="a"/><connRef v="20"/></connection><connection><identifier v="b"/><connRef v="21"/></connection></connections></paramNode>
        </paramNodes>
      </dynamicValue></paramValue></parameter></parameters></compFilter></compImplementation>
    </compNode></compNodes></graph></content></package>"#;
    let read = read_sbs(xml.as_bytes(), "values.sbs", &no_linked_files).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    let back = read_sbs(&write_sbs(&read.pkg).unwrap(), "values.sbs", &no_linked_files).unwrap();
    let Value::Function(function) = &back.pkg.graphs[0].nodes[0].params["function"] else { panic!("missing function") };

    assert_eq!(function.nodes[0].value, Some(Value::Int(16_777_217)));
    assert_eq!(function.nodes[2].op, "add");
    assert_eq!(function.nodes[2].type_code, 16);
    assert_eq!(function.nodes[2].inputs, vec![Some(function.nodes[0].id), Some(function.nodes[1].id)]);
}

#[test]
fn distance_metric_survives_a_package_round_trip() {
    let mut package = Package::new("Distances");
    let mut graph = Graph::new("Distances");
    for metric in [1, 2] {
        let node = graph.add_node("distance", [0.0; 2]);
        graph.node_mut(node).unwrap().params.insert("metric".into(), Value::Enum(metric));
    }
    package.graphs.push(graph);
    let bytes = write_sbs(&package).unwrap();
    let document = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(document.descendants().filter(|n| n.has_tag_name("name") && n.attribute("v") == Some("distancemode")).count(), 2);
    let read = read_sbs(&bytes, "distances.sbs", &no_linked_files).unwrap();
    let metrics: Vec<_> = read.pkg.graphs[0].nodes.iter().filter_map(|n| n.params.get("metric").and_then(Value::as_i32)).collect();
    assert_eq!(metrics, [1, 2]);
}

#[test]
fn distance_preserves_declared_grayscale_and_color_outputs_when_the_switch_is_omitted() {
    let xml = br#"<package><content><graph><identifier v="Distance outputs"/><compNodes>
        <compNode><uid v="10"/><compOutputs><compOutput><uid v="11"/><comptype v="1"/></compOutput></compOutputs>
        <compImplementation><compFilter><filter v="distance"/></compFilter></compImplementation></compNode>
        <compNode><uid v="20"/><compOutputs><compOutput><uid v="21"/><comptype v="2"/></compOutput></compOutputs>
        <compImplementation><compFilter><filter v="distance"/></compFilter></compImplementation></compNode>
        </compNodes></graph></content></package>"#;
    let loaded = read_sbs(xml, "distances.sbs", &no_linked_files).unwrap().pkg;
    let saved = write_sbs(&loaded).unwrap();
    let again = read_sbs(&saved, "distances.sbs", &no_linked_files).unwrap().pkg;
    for package in [&loaded, &again] {
        let colors: Vec<_> = package.graphs[0].nodes.iter().filter_map(|node| node.params.get("color_mode").and_then(Value::as_bool)).collect();
        assert_eq!(colors, [true, false]);
    }
}

#[test]
fn image_output_types_follow_color_mode_and_blended_inputs() {
    let mut package = Package::new("Channels");
    let mut graph = Graph::new("Channels");
    let gray = graph.add_node("uniform", [0.0; 2]);
    graph.node_mut(gray).unwrap().params.insert("color_mode".into(), Value::Bool(false));
    let color = graph.add_node("uniform", [0.0; 2]);
    let blend = graph.add_node("blend", [0.0; 2]);
    let output = graph.add_node("output", [0.0; 2]);
    graph.connect(gray, "output", blend, "foreground").unwrap();
    graph.connect(color, "output", blend, "background").unwrap();
    graph.connect(blend, "output", output, "input").unwrap();
    package.graphs.push(graph);
    let bytes = write_sbs(&package).unwrap();
    let document = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    let types: Vec<_> = document.descendants().filter(|node| node.has_tag_name("comptype")).map(|node| node.attribute("v").unwrap()).collect();
    assert_eq!(types, ["2", "1", "1"]);
    assert_eq!(document.descendants().find(|node| node.has_tag_name("channels")).unwrap().attribute("v"), Some("1"));
}

#[test]
fn navigation_pins_round_trip_in_graphs_and_functions() {
    let canvas = r#"<GUIObjects>
        <GUIObject><type v="PIN"/><GUILayout><gpos v="-128 64 0"/></GUILayout><GUIName v="Start &amp; &lt;review&gt;"/><uid v="50"/></GUIObject>
        <GUIObject><type v="PIN"/><GUILayout><gpos v="512 -96 0"/></GUILayout><GUIName v="Finish"/><uid v="51"/></GUIObject>
    </GUIObjects>"#;
    let items = [
        format!("<graph><identifier v=\"Pins\"/>{canvas}</graph>"),
        format!("<function><identifier v=\"Pins\"/><paramValue><dynamicValue><paramNodes/>{canvas}</dynamicValue></paramValue></function>"),
    ];

    for item in items {
        let xml = format!("<package><content>{item}</content></package>");
        let imported = read_sbs(xml.as_bytes(), "pins.sbs", &no_linked_files).unwrap();
        assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
        let pins = &imported.pkg.graphs[0].pins;
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].name, "Start & <review>");
        assert_eq!(pins[0].pos, [-128.0, 64.0]);
        assert_eq!(pins[1].name, "Finish");
        assert_eq!(pins[1].pos, [512.0, -96.0]);
        assert_ne!(pins[0].id, pins[1].id);

        let saved = write_sbs(&imported.pkg).unwrap();
        let native = roxmltree::Document::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
        let pin_count = native.descendants().filter(|element| element.has_tag_name("type") && element.attribute("v") == Some("PIN")).count();
        assert_eq!(pin_count, 2);

        let reopened = read_sbs(&saved, "pins.sbs", &no_linked_files).unwrap();
        assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
        assert_eq!(reopened.pkg.graphs[0].pins, *pins);
    }
}

#[test]
fn component_usages_round_trip_without_reassigning_existing_indices() {
    use materialcraft_core::ComponentUsage;

    let original = ["baseColor", "normal", "roughness", "metallic", "height", "ambientOcclusion", "emissive", "opacity", "specular", "any"];
    for (index, id) in original.iter().enumerate() {
        assert_eq!(ComponentUsage::from_index(index as u32).id, *id);
    }
    assert_eq!(ComponentUsage::from_index(u32::MAX), ComponentUsage::ANY);
    assert_eq!(ComponentUsage::index("futureUsage"), ComponentUsage::ANY_INDEX);
    assert_eq!(ComponentUsage::index(" DIFFUSE "), ComponentUsage::index("diffuse"));

    let mut package = Package::new("Usage round trip");
    let mut graph = Graph::new("Usages");
    for definition in ["output", "input_color", "input_grayscale", "input_value"] {
        for (index, usage) in ComponentUsage::ALL.iter().enumerate() {
            let id = graph.add_node(definition, [index as f32 * 160.0, 0.0]);
            graph
                .node_mut(id)
                .unwrap()
                .params
                .extend([("identifier".into(), Value::Text(format!("{definition}_{}", usage.id))), ("usage".into(), Value::Enum(index as u32))]);
        }
    }
    package.graphs.push(graph);
    let xml = write_sbs(&package).unwrap();
    let restored = read_sbs(&xml, "usages.sbs", &no_linked_files).unwrap();
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert_eq!(restored.pkg.graphs[0].nodes.len(), package.graphs[0].nodes.len());
    for (original, node) in package.graphs[0].nodes.iter().zip(&restored.pkg.graphs[0].nodes) {
        assert_eq!(node.def, original.def);
        assert_eq!(node.params.get("usage"), original.params.get("usage"));
        assert_eq!(node.params.get("identifier"), original.params.get("identifier"));
    }
}

#[test]
fn input_usage_defaults_and_unknown_metadata_stay_unclassified() {
    use materialcraft_core::ComponentUsage;

    let mut package = Package::new("Input usages");
    let mut graph = Graph::new("Inputs");
    for definition in ["input_color", "input_grayscale", "input_value"] {
        graph.add_node(definition, [0.0; 2]);
    }
    package.graphs.push(graph);
    let saved = String::from_utf8(write_sbs(&package).unwrap()).unwrap();
    let usage = "<usages><usage><components v=\"RGBA\"/><name v=\"any\"/></usage></usages>";
    assert_eq!(saved.matches(usage).count(), 3);
    for xml in [saved.clone(), saved.replace(usage, ""), saved.replace("<name v=\"any\"/>", "<name v=\"futureUsage\"/>")] {
        let restored = read_sbs(xml.as_bytes(), "inputs.sbs", &no_linked_files).unwrap();
        assert_eq!(restored.pkg.graphs[0].nodes.len(), 3);
        for node in &restored.pkg.graphs[0].nodes {
            assert_eq!(node.params.get("usage"), Some(&Value::Enum(ComponentUsage::ANY_INDEX)));
        }
    }
}

#[test]
fn graph_seed_inheritance_survives_sbs_saving() {
    for mode in [None, Some(0), Some(1), Some(2)] {
        for seed in [0, 5, -3] {
            if mode.is_none() && seed != 0 {
                continue;
            }
            let parameter = mode.map_or_else(String::new, |mode| {
                format!(r#"<parameter><name v="randomseed"/><relativeTo v="{mode}"/><paramValue><constantValueInt32 v="{seed}"/></paramValue></parameter>"#)
            });
            let xml = format!(r#"<package><content><graph><identifier v="Field"/><baseParameters>{parameter}</baseParameters></graph></content></package>"#);
            let package = read_sbs(xml.as_bytes(), "field.sbs", &no_linked_files).unwrap().pkg;
            let bytes = write_sbs(&package).unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            let document = roxmltree::Document::parse(text).unwrap();
            let base = document.descendants().find(|n| n.has_tag_name("baseParameters")).unwrap();
            let saved = super::base_parameters::read_base_params(Some(base), &mut Default::default());
            use materialcraft_core::ParameterInheritance::{Absolute, RelativeToInput, RelativeToParent};
            let expected = match mode {
                Some(0) => Absolute,
                Some(1) => RelativeToParent,
                _ => RelativeToInput,
            };
            assert_eq!((saved.seed_mode, saved.seed), (expected, seed), "source {mode:?} {seed}");
        }
    }
}

#[test]
fn graph_seed_functions_round_trip_with_their_inheritance() {
    use materialcraft_core::BaseParameter;
    let xml = r#"<package><content><graph><identifier v="Field"/><baseParameters>
      <parameter><name v="randomseed"/><relativeTo v="1"/><paramValue><dynamicValue>
        <rootnode v="21"/><paramNodes><paramNode><uid v="21"/><function v="const_int1"/><type v="16"/>
        <funcDatas><funcData><name v="const_int1"/><constantValue><constantValueInt1 v="5"/></constantValue></funcData></funcDatas>
        </paramNode></paramNodes></dynamicValue></paramValue></parameter></baseParameters></graph></content></package>"#;
    let package = read_sbs(xml.as_bytes(), "seed.sbs", &no_linked_files).unwrap().pkg;
    let saved = write_sbs(&package).unwrap();
    let document = roxmltree::Document::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
    let element = document.descendants().find(|n| n.has_tag_name("baseParameters")).unwrap();
    let base = super::base_parameters::read_base_params(Some(element), &mut Default::default());
    assert_eq!(base.seed_mode, materialcraft_core::ParameterInheritance::RelativeToParent);
    let function = base.functions.get(&BaseParameter::RandomSeed).expect("the saved seed has its authored function");
    assert_eq!(function.node(function.output.unwrap()).unwrap().value, Some(materialcraft_core::Value::Int(5)));
    assert_eq!(read_sbs(&saved, "copy.sbs", &no_linked_files).unwrap().pkg.graphs[0].functions().count(), 1);
}

#[test]
fn relative_graph_size_offsets_keep_their_axes_and_inheritance_on_save() {
    for (mode, axes) in [(1, [-1, 1]), (2, [1, -1]), (1, [0, 0]), (2, [-6, 0])] {
        let [x, y] = axes;
        let xml = format!(
            r#"<package><content><graph><identifier v="Field"/><baseParameters><parameter><name v="outputsize"/><relativeTo v="{mode}"/><paramValue><constantValueInt2 v="{x} {y}"/></paramValue></parameter></baseParameters></graph></content></package>"#
        );
        let pkg = read_sbs(xml.as_bytes(), "field.sbs", &no_linked_files).unwrap().pkg;
        let saved = write_sbs(&pkg).unwrap();
        let doc = roxmltree::Document::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
        let element = doc.descendants().find(|n| n.has_tag_name("baseParameters")).unwrap();
        let base = super::base_parameters::read_base_params(Some(element), &mut Default::default());
        let expected = if mode == 1 { materialcraft_core::SizeMode::RelativeToParent } else { materialcraft_core::SizeMode::RelativeToInput };
        assert_eq!((base.size_mode, base.size), (expected, axes));
        assert_eq!(read_sbs(&saved, "copy.sbs", &no_linked_files).unwrap().pkg.graphs[0].size_px(), pkg.graphs[0].size_px());
    }
}

#[test]
fn computed_graph_output_sizes_round_trip_with_their_inheritance() {
    use materialcraft_core::{BaseParameter, SizeMode, Value};
    for (raw, expected) in [(0, SizeMode::Absolute), (1, SizeMode::RelativeToParent), (2, SizeMode::RelativeToInput)] {
        let xml = format!(
            r#"<package><content><graph><identifier v="Dimensions"/><baseParameters>
          <parameter><name v="outputsize"/><relativeTo v="{raw}"/><paramValue><dynamicValue>
            <rootnode v="41"/><paramNodes><paramNode><uid v="41"/><function v="const_int2"/><type v="32"/>
            <funcDatas><funcData><name v="const_int2"/><constantValue><constantValueInt2 v="-1 1"/></constantValue></funcData></funcDatas>
            </paramNode></paramNodes></dynamicValue></paramValue></parameter></baseParameters></graph></content></package>"#
        );
        let package = read_sbs(xml.as_bytes(), "dimensions.sbs", &no_linked_files).unwrap().pkg;
        let saved = write_sbs(&package).unwrap();
        let document = roxmltree::Document::parse(std::str::from_utf8(&saved).unwrap()).unwrap();
        let element = document.descendants().find(|n| n.has_tag_name("baseParameters")).unwrap();
        let base = super::base_parameters::read_base_params(Some(element), &mut Default::default());
        let function = base.functions.get(&BaseParameter::OutputSize).expect("the saved size has its authored function");
        assert_eq!(base.size_mode, expected);
        assert_eq!(function.node(function.output.unwrap()).unwrap().value, Some(Value::Int2([-1, 1])));
        assert_eq!(read_sbs(&saved, "copy.sbs", &no_linked_files).unwrap().pkg.graphs[0].functions().count(), 1);
    }
}

#[test]
fn published_material_preserves_disabled_nodes_and_localized_typed_interfaces() {
    use crate::material::{read_material, write_material};
    use materialcraft_core::{GraphInstance, GraphPort, ValueType};

    let mut helper = Graph::new("Helper");
    let input = helper.add_node("input_grayscale", [0.0; 2]);
    helper.node_mut(input).unwrap().params.insert("identifier".into(), Value::Text("gray".into()));
    let out = helper.add_node("output", [0.0; 2]);
    helper.node_mut(out).unwrap().params.insert("identifier".into(), Value::Text("result".into()));
    helper.connect(input, "output", out, "input").unwrap();
    let mut dependency = Package::new("Dependency");
    dependency.graphs.push(helper);

    let mut graph = Graph::new("Root");
    let uniform = graph.add_node("uniform", [0.0; 2]);
    let levels = graph.add_node("levels", [0.0; 2]);
    graph.node_mut(levels).unwrap().disabled = true;
    graph.connect(uniform, "output", levels, "input").unwrap();
    let instance = graph.add_node("graph_instance", [0.0; 2]);
    let inputs = vec![GraphPort { identifier: "gray".into(), type_code: 2 }, GraphPort { identifier: "unused<&".into(), type_code: ValueType::Float as u32 }];
    graph.node_mut(instance).unwrap().instance = Some(GraphInstance {
        package: "dependency.sbs".into(),
        graph: "Helper".into(),
        inputs: inputs.clone(),
        outputs: vec![GraphPort { identifier: "result".into(), type_code: 2 }],
        defaults: Default::default(),
        label: String::new(),
    });
    graph.connect(levels, "output", instance, "gray").unwrap();
    let out = graph.add_node("output", [0.0; 2]);
    graph.connect(instance, "result", out, "input").unwrap();
    let mut package = Package::new("Package");
    package.graphs.push(graph);
    package.dependencies.insert("dependency.sbs".into(), std::sync::Arc::new(dependency));

    let bytes = write_material(&package, 0).unwrap();
    let (opened, entry) = read_material(&bytes).unwrap();
    let graph = &opened.graphs[entry];
    assert!(graph.nodes.iter().find(|node| node.def == "levels").unwrap().disabled);
    assert!(!graph.nodes.iter().find(|node| node.def == "uniform").unwrap().disabled);
    let node = graph.nodes.iter().find(|node| node.def == "graph_instance").unwrap();
    let restored = node.instance.as_ref().expect("localization must retain the typed interface");
    assert!(restored.package.is_empty());
    assert_eq!(restored.inputs, inputs);
    assert_eq!(restored.outputs, vec![GraphPort { identifier: "result".into(), type_code: 2 }]);
    assert!(opened.graph_by_name(&restored.graph).is_some());
    assert!(graph.connections.iter().any(|wire| wire.to == node.id && wire.to_port == "gray"));

    // Republish the already-localized graph to exercise local interfaces
    let bytes = write_material(&opened, entry).unwrap();
    let (reopened, entry) = read_material(&bytes).unwrap();
    assert_eq!(reopened.graphs[entry].nodes.iter().find_map(|node| node.instance.as_ref()).unwrap().inputs, inputs);
    assert!(reopened.graphs[entry].nodes.iter().find(|node| node.def == "levels").unwrap().disabled);
}

#[test]
fn inferred_instance_inputs_respect_the_typed_interface_limit() {
    use materialcraft_core::{GraphInstance, GraphPort};

    let mut package = Package::new("Package");
    let mut root = Graph::new("Root");
    let source = root.add_node("uniform", [0.0; 2]);
    let instance = root.add_node("graph_instance", [0.0; 2]);
    let inputs = (0..4096).map(|index| GraphPort { identifier: format!("input{index}"), type_code: 1 }).collect();
    root.node_mut(instance).unwrap().instance =
        Some(GraphInstance { label: String::new(), package: String::new(), graph: "Helper".into(), inputs, outputs: Vec::new(), defaults: Default::default() });
    root.connect(source, "output", instance, "input0").unwrap();
    package.graphs.extend([root, Graph::new("Helper")]);
    let xml = String::from_utf8(write_sbs(&package).unwrap()).unwrap();
    let xml = xml.replace("<connection><identifier v=\"input0\"", "<connection><identifier v=\"inferred\"");

    let restored = read_sbs(xml.as_bytes(), "instance.sbs", &no_linked_files).unwrap();

    let root = &restored.pkg.graphs[0];
    assert_eq!(root.instances().next().unwrap().inputs.len(), 4096);
    assert!(root.connections.is_empty());
    assert!(restored.warnings.iter().any(|warning| warning.contains("4096-input")));
}
