use craft_texture::Image;
use materialcraft_core::{OutputFormat, Value};
use materialcraft_formats::sbs::{read_sbs, write_sbs};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::collections::BTreeMap;

fn curve_package(cells: impl IntoIterator<Item = String>) -> String {
    let cells: String = cells.into_iter().collect();
    format!(
        r#"<package><content><graph><identifier v="Tone"/><compNodes><compNode><uid v="1"/><compImplementation><compFilter><filter v="curve"/>
        <paramsArrays><paramsArray><name v="curveluminance"/><paramsArrayCells>{cells}</paramsArrayCells></paramsArray></paramsArrays>
        </compFilter></compImplementation></compNode></compNodes></graph></content></package>"#
    )
}

fn cell(position: [f32; 2], left: [f32; 2], right: [f32; 2], flags: [bool; 3]) -> String {
    let mut parameters = String::new();
    for (name, [x, y]) in [("position", position), ("left", left), ("right", right)] {
        parameters.push_str(&format!(r#"<parameter><name v="{name}"/><paramValue><constantValueFloat2 v="{x} {y}"/></paramValue></parameter>"#));
    }
    for (name, flag) in ["isLeftBroken", "isRightBroken", "isLocked"].into_iter().zip(flags) {
        parameters.push_str(&format!(r#"<parameter><name v="{name}"/><paramValue><constantValueBool v="{}"/></paramValue></parameter>"#, u8::from(flag)));
    }
    format!("<paramsArrayCell><parameters>{parameters}</parameters></paramsArrayCell>")
}

#[test]
fn curve_tables_preserve_dormant_handles_and_their_flags_on_save() {
    let xml = curve_package([
        cell([0.0, 0.0625], [0.0, 0.0625], [0.1875, -0.25], [false, true, true]),
        cell([1.0, 0.8125], [0.578125, 0.9375], [1.0, 0.8125], [false; 3]),
    ]);
    let mut bytes = xml.into_bytes();
    for _ in 0..2 {
        let package = read_sbs(&bytes, "curve.sbs", &|_| None).unwrap().pkg;
        let Value::Curve(curve) = &package.graphs[0].nodes[0].params["curve"] else { panic!("missing curve") };
        assert_eq!(curve.points[0].right, Some([0.1875, -0.25]));
        assert_eq!(curve.segment(0).unwrap(), [[0.0, 0.0625], [0.0, 0.0625], [0.578125, 0.9375], [1.0, 0.8125]]);
        bytes = write_sbs(&package).unwrap();
        let doc = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let flags: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("constantValueBool"))
            .filter_map(|n| {
                let name = n.parent()?.parent()?.children().find(|c| c.has_tag_name("name"))?.attribute("v")?;
                matches!(name, "isLeftBroken" | "isRightBroken" | "isLocked").then_some((name, n.attribute("v")?))
            })
            .collect();
        assert_eq!(flags, [("isLeftBroken", "0"), ("isRightBroken", "1"), ("isLocked", "1"), ("isLeftBroken", "0"), ("isRightBroken", "0"), ("isLocked", "0")]);
    }
}

#[test]
fn curve_tables_sample_each_segment_even_when_anchors_share_a_texel() {
    let points = [[0.0, 0.75], [0.41031, 0.25], [0.41036, 1.125], [0.4104, -0.0625], [1.0, 0.875]];
    let xml = curve_package(points.map(|[x, y]| cell([x, y], [x - 0.03125, y], [x + 0.046875, y], [true, true, false])));
    let package = read_sbs(xml.as_bytes(), "curve.sbs", &|_| None).unwrap().pkg;
    let definition = find("curve").unwrap();
    let params = resolve_params(definition, &package.graphs[0].nodes[0].params);
    let values = [0x3ed2348d, 0x3ed2b4ad, 0x3ed334cd, 0x3e1779c3, 0x3e931d48];
    let inputs = [Some(Image::gray(5, 1, |x, _| f32::from_bits(values[x as usize])))];
    let vars = BTreeMap::new();
    let context = CookContext {
        w: 5,
        h: 1,
        seed: 0,
        format: OutputFormat::F32,
        input_formats: &[OutputFormat::F32],
        tile: true,
        inputs: &inputs,
        params: &params,
        resource: &|_| None,
        vars: &vars,
    };
    let image = (definition.cook)(&context).unwrap().remove(0);
    assert_eq!(image.data().iter().map(|x| x.to_bits()).collect::<Vec<_>>(), [0x3e800000, 0x41ad6dd8, 0xc27b1ba9, 0x3f11da69, 0x3eccba50]);
}
