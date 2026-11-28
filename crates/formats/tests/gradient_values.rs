use materialcraft_core::Value;
use materialcraft_formats::sbs::{read_sbs, write_sbs};

#[test]
fn an_omitted_native_uniform_color_is_black() {
    let xml = br#"<package><content><graph><identifier v="Noise"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="uniform"/></compFilter></compImplementation>
        </compNode></compNodes></graph></content></package>"#;
    let read = read_sbs(xml, "noise.sbs", &|_| None).unwrap();

    assert_eq!(read.pkg.graphs[0].nodes[0].params["color"], Value::Color([0.0, 0.0, 0.0, 1.0]));

    let saved = write_sbs(&read.pkg).unwrap();
    let reopened = read_sbs(&saved, "saved.sbs", &|_| None).unwrap();

    assert_eq!(reopened.pkg.graphs[0].nodes[0].params["color"], read.pkg.graphs[0].nodes[0].params["color"]);
}

#[test]
fn native_gradient_default_midpoints_are_linear_and_components_are_preserved() {
    for color in [false, true] {
        let array = if color { "gradientrgba" } else { "gradientluminance" };
        let value = if color { "constantValueFloat4 v=\"0.5 0.25 0.75 0.4\"" } else { "constantValueFloat1 v=\"0.5\"" };
        let zero = if color { "constantValueFloat4 v=\"0 0 0 0.4\"" } else { "constantValueFloat1 v=\"0\"" };
        let xml = format!(
            r#"<package><content><graph><identifier v="Ramp"/><compNodes><compNode><uid v="1"/>
            <compImplementation><compFilter><filter v="gradient"/><parameters>
            <parameter><name v="colorswitch"/><paramValue><constantValueBool v="{}"/></paramValue></parameter>
            </parameters><paramsArrays><paramsArray><name v="{array}"/><paramsArrayCells>
            <paramsArrayCell><parameters>
            <parameter><name v="position"/><paramValue><constantValueFloat1 v="0"/></paramValue></parameter>
            <parameter><name v="midpoint"/><paramValue><constantValueFloat1 v="0"/></paramValue></parameter>
            <parameter><name v="value"/><paramValue><{zero}/></paramValue></parameter>
            </parameters></paramsArrayCell><paramsArrayCell><parameters>
            <parameter><name v="position"/><paramValue><constantValueFloat1 v="1"/></paramValue></parameter>
            <parameter><name v="midpoint"/><paramValue><constantValueFloat1 v="0"/></paramValue></parameter>
            <parameter><name v="value"/><paramValue><{value}/></paramValue></parameter>
            </parameters></paramsArrayCell></paramsArrayCells></paramsArray></paramsArrays>
            </compFilter></compImplementation></compNode></compNodes></graph></content></package>"#,
            u8::from(color)
        );
        let mut read = read_sbs(xml.as_bytes(), "ramp.sbs", &|_| None).unwrap();
        for _ in 0..3 {
            assert!(read.warnings.is_empty(), "{:?}", read.warnings);

            let Value::Gradient(gradient) = &read.pkg.graphs[0].nodes[0].params["gradient"] else { panic!("missing gradient") };
            let end = 0.5;

            assert!((gradient.eval(1.0)[0] - end).abs() < 1e-6);
            assert!((gradient.eval(0.5)[0] - end * 0.5).abs() < 1e-6);
            assert!((gradient.eval(0.5)[3] - if color { 0.4 } else { 1.0 }).abs() < 1e-6);

            let saved = write_sbs(&read.pkg).unwrap();
            let xml = std::str::from_utf8(&saved).unwrap();
            assert!(xml.contains(r#"<name v="gradientrgba"/>"#));
            assert_eq!(xml.matches("<constantValueFloat4 ").count(), 2);
            read = read_sbs(&saved, "saved.sbs", &|_| None).unwrap();
        }
    }
}

#[test]
fn smooth_gradient_signed_bias_survives_sbs_round_trip() {
    let xml = br#"<package><content><graph><identifier v="Ramp"/><compNodes><compNode><uid v="1"/>
        <compImplementation><compFilter><filter v="gradient"/><parameters>
        <parameter><name v="interpmode"/><paramValue><constantValueInt32 v="1"/></paramValue></parameter>
        </parameters><paramsArrays><paramsArray><name v="gradientrgba"/><paramsArrayCells>
        <paramsArrayCell><parameters><parameter><name v="midpoint"/>
        <paramValue><constantValueFloat1 v="-0.375"/></paramValue></parameter></parameters></paramsArrayCell>
        </paramsArrayCells></paramsArray></paramsArrays></compFilter>
        </compImplementation></compNode></compNodes></graph></content></package>"#;
    let mut read = read_sbs(xml, "ramp.sbs", &|_| None).unwrap();
    for _ in 0..2 {
        let params = &read.pkg.graphs[0].nodes[0].params;
        assert_eq!(params["interpolation"].as_i32(), Some(1));
        let Value::Gradient(gradient) = &params["gradient"] else { panic!("missing gradient") };
        assert_eq!(gradient.keys[0].midpoint, 0.3125);
        let saved = write_sbs(&read.pkg).unwrap();
        read = read_sbs(&saved, "ramp.sbs", &|_| None).unwrap();
    }
}
