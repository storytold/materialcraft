use materialcraft_formats::sbs::{read_sbs, write_sbs};

#[test]
fn grayscale_channel_weights_round_trip_as_one_function() {
    use materialcraft_core::Value;
    let xml = br#"<package><content><graph><identifier v="Weights"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="grayscaleconversion"/><parameters>
        <parameter><name v="channelsweights"/><paramValue><dynamicValue><rootnode v="17"/><paramNodes>
        <paramNode><uid v="17"/><function v="const_float4"/><type v="2048"/><funcDatas><funcData><name v="const_float4"/>
        <constantValue><constantValueFloat4 v="0.125 0.25 0.5 0.125"/></constantValue></funcData></funcDatas></paramNode>
        </paramNodes></dynamicValue></paramValue></parameter></parameters></compFilter></compImplementation>
        </compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "weights.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        let node = &read.pkg.graphs[0].nodes[0];
        assert_eq!(node.functions.len(), 1);
        assert_eq!(node.functions["weights"].nodes[0].value, Some(Value::Float4([0.125, 0.25, 0.5, 0.125])));
        let saved = write_sbs(&read.pkg).unwrap();
        assert_eq!(std::str::from_utf8(&saved).unwrap().matches(r#"<name v="channelsweights"/>"#).count(), 1);
        read = read_sbs(&saved, "saved.sbs", &|_| None).unwrap();
    }
}

#[test]
fn levels_preserves_the_unclamped_input_range_on_save() {
    let xml = br#"<package><content><graph><identifier v="Range"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="levels"/><parameters><parameter><name v="clampinterm"/>
        <paramValue><constantValueBool v="0"/></paramValue></parameter></parameters></compFilter></compImplementation>
        </compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "range.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert_eq!(read.pkg.graphs[0].nodes[0].params["clamp_intermediate"].as_bool(), Some(false));
        let saved = write_sbs(&read.pkg).unwrap();
        read = read_sbs(&saved, "saved.sbs", &|_| None).unwrap();
    }
}

#[test]
fn fx_map_render_and_tiling_regions_keep_native_names_and_functions() {
    use materialcraft_core::Value;
    let xml = br#"<package><content><graph><identifier v="Regions"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="fxmaps"/><parameters>
        <parameter><name v="renderroi"/><paramValue><dynamicValue><rootnode v="2"/><paramNodes>
        <paramNode><uid v="2"/><function v="const_float4"/><funcDatas><funcData><name v="const_float4"/>
        <constantValueFloat4 v="0 0.5 0 1"/></funcData></funcDatas></paramNode></paramNodes></dynamicValue></paramValue></parameter>
        <parameter><name v="tilingroi"/><paramValue><constantValueFloat4 v="0 2 0 1"/></paramValue></parameter>
        <parameter><name v="culling"/><paramValue><constantValueBool v="1"/></paramValue></parameter>
        </parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "regions.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);

        let node = &read.pkg.graphs[0].nodes[0];

        assert_eq!(node.params["cull_outside"], Value::Bool(true));
        assert_eq!(node.params["tiling_region"], Value::Float4([0.0, 2.0, 0.0, 1.0]));
        assert_eq!(node.functions["render_region"].nodes[0].value, Some(Value::Float4([0.0, 0.5, 0.0, 1.0])));

        let bytes = write_sbs(&read.pkg).unwrap();
        let saved = std::str::from_utf8(&bytes).unwrap();

        assert!(saved.contains("<name v=\"culling\"/>"));
        assert!(saved.contains("<name v=\"renderroi\"/>") && saved.contains("<name v=\"tilingroi\"/>"));

        read = read_sbs(&bytes, "saved.sbs", &|_| None).unwrap();
    }
}

#[test]
fn native_dynamic_gradient_coordinates_and_orientation_survive_saving() {
    let xml = br#"<package><content><graph><identifier v="Gradient"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="dyngradient"/><parameters>
        <parameter><name v="coordinate"/><paramValue><dynamicValue><rootnode v="2"/><paramNodes>
        <paramNode><uid v="2"/><function v="const_float1"/><funcDatas><funcData><name v="const_float1"/>
        <constantValueFloat1 v="0.25"/></funcData></funcDatas></paramNode></paramNodes></dynamicValue></paramValue></parameter>
        <parameter><name v="uvselector"/><paramValue><constantValueInt32 v="1"/></paramValue></parameter>
        </parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "gradient.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);

        let node = &read.pkg.graphs[0].nodes[0];

        assert_eq!(node.params["orientation"], materialcraft_core::Value::Enum(1));
        assert_eq!(node.functions["position"].nodes[0].value, Some(materialcraft_core::Value::Float(0.25)));

        let bytes = write_sbs(&read.pkg).unwrap();
        let saved = std::str::from_utf8(&bytes).unwrap();

        assert!(saved.contains("<name v=\"coordinate\"/>") && saved.contains("<name v=\"uvselector\"/>"));

        read = read_sbs(&bytes, "saved.sbs", &|_| None).unwrap();
    }

    let legacy = std::str::from_utf8(xml).unwrap().replace("coordinate", "position").replace("uvselector", "orientation");

    assert!(read_sbs(legacy.as_bytes(), "legacy.sbs", &|_| None).unwrap().warnings.is_empty());
}

#[test]
fn linked_resources_retain_the_successful_path_but_embedded_resources_do_not_link_stale_files() {
    let xml = br#"<package><content><resource><identifier v="Font"/><type v="font"/><format v="ttf"/>
        <filepath v="resources/example.ttf"/><source><externalcopy><filename v="absent.ttf"/></externalcopy></source>
        </resource></content></package>"#;
    let read = read_sbs(xml, "example.sbs", &|path| (path == "resources/example.ttf").then(|| vec![1, 2, 3])).unwrap();

    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert_eq!(read.pkg.resources[0].source, "resources/example.ttf");

    let saved = write_sbs(&read.pkg).unwrap();
    let saved = String::from_utf8(saved).unwrap();

    assert!(saved.contains("<type v=\"font\"/>"));

    let saved = saved.replace("<filepath v=\"\"/>", "<filepath v=\"fonts/cour.ttf\"/>");
    let embedded = read_sbs(saved.as_bytes(), "saved.sbs", &|_| None).unwrap();

    assert!(embedded.warnings.is_empty(), "{:?}", embedded.warnings);
    assert!(embedded.pkg.resources[0].source.is_empty());
    assert_eq!(embedded.pkg.resources[0].bytes.as_slice(), &[1, 2, 3]);
}

/// Native pattern inputs and HSL lightness must survive conversion without dropped wires/defaults.
#[test]
fn native_pattern_inputs_and_hsl_luminosity_round_trip() {
    let xml = r#"<package><content><graph><identifier v="Example"/><compNodes>
        <compNode><uid v="1"/><compImplementation><compFilter><filter v="uniform"/></compFilter></compImplementation>
        <compOutputs><compOutput><uid v="11"/><identifier v="output"/></compOutput></compOutputs></compNode>
        <compNode><uid v="2"/><compImplementation><compFilter><filter v="fxmaps"/></compFilter></compImplementation>
        <connections><connection><identifier v="inputpattern:2"/><connRef v="1"/><connRefOutput v="11"/></connection></connections></compNode>
        <compNode><uid v="3"/><compImplementation><compFilter><filter v="hsl"/><parameters>
        <parameter><name v="luminosity"/><paramValue><constantValueFloat1 v="0.75"/></paramValue></parameter>
        </parameters></compFilter></compImplementation></compNode>
        </compNodes></graph></content></package>"#;
    let read = read_sbs(xml.as_bytes(), "example.sbs", &|_| None).unwrap();

    assert!(read.warnings.is_empty(), "{:?}", read.warnings);

    let graph = &read.pkg.graphs[0];

    assert_eq!(graph.connections.len(), 1);
    assert_eq!(graph.connections[0].to_port, "pattern2");
    assert_eq!(graph.nodes.iter().find(|node| node.def == "hsl").unwrap().params["lightness"], materialcraft_core::Value::Float(0.75));

    let saved = write_sbs(&read.pkg).unwrap();

    assert!(std::str::from_utf8(&saved).unwrap().contains("inputpattern:2"));
    assert!(std::str::from_utf8(&saved).unwrap().contains("<name v=\"luminosity\"/>"));
    assert!(!std::str::from_utf8(&saved).unwrap().contains("<name v=\"luminance\"/>"));

    let reopened = read_sbs(&saved, "saved.sbs", &|_| None).unwrap();

    assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
    assert_eq!(reopened.pkg.graphs[0].connections.len(), 1);

    let legacy = xml.replace("luminosity", "luminance").replace("inputpattern:2", "input:2");
    let legacy = read_sbs(legacy.as_bytes(), "legacy.sbs", &|_| None).unwrap();

    assert!(legacy.warnings.is_empty(), "{:?}", legacy.warnings);
    assert_eq!(legacy.pkg.graphs[0].connections.len(), 1);
    assert_eq!(legacy.pkg.graphs[0].nodes.iter().find(|node| node.def == "hsl").unwrap().params["lightness"], materialcraft_core::Value::Float(0.75));
}
#[test]
fn warp_filtering_round_trips_as_a_boolean() {
    let xml = br#"<package><content><graph><identifier v="Warp"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="warp"/><parameters>
        <parameter><name v="warpfilter"/><paramValue><constantValueBool v="1"/></paramValue></parameter>
        </parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "warp.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert_eq!(read.pkg.graphs[0].nodes[0].params.get("nearest"), Some(&materialcraft_core::Value::Bool(true)));
        let bytes = write_sbs(&read.pkg).unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("<name v=\"warpfilter\"/>"));
        read = read_sbs(&bytes, "saved.sbs", &|_| None).unwrap();
    }
}

#[test]
fn transform_mip_settings_survive_sbs_round_trip() {
    let xml = br#"<package><content><graph><identifier v="Mip"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="transformation"/><parameters>
        <parameter><name v="mipmapmode"/><paramValue><constantValueInt32 v="1"/></paramValue></parameter>
        <parameter><name v="manualmiplevel"/><paramValue><constantValueInt32 v="3"/></paramValue></parameter>
        </parameters></compFilter></compImplementation></compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "mip.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        let params = &read.pkg.graphs[0].nodes[0].params;
        assert_eq!(params.get("mip_mode"), Some(&materialcraft_core::Value::Enum(1)));
        assert_eq!(params.get("mip_level"), Some(&materialcraft_core::Value::Int(3)));
        let bytes = write_sbs(&read.pkg).unwrap();
        read = read_sbs(&bytes, "saved.sbs", &|_| None).unwrap();
    }
}
