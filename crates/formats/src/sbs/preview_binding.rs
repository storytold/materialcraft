use super::xml::{child_value, xml_escape};
use materialcraft_core::Graph;
use roxmltree::Node as XmlNode;
use std::fmt::Write as _;

const IMAGE_PREFIX: &str = "materialcraft.previewImage.";
const LINEAR_PREFIX: &str = "materialcraft.previewLinear.";

pub(super) fn write(out: &mut String, graph: &Graph) {
    for (identifier, resource) in &graph.input_images {
        if graph.input_images_from_file.contains(identifier) {
            continue;
        }
        let _ = write!(out, "<option><name v=\"{IMAGE_PREFIX}{}\"/><value v=\"{}\"/></option>", xml_escape(identifier), xml_escape(resource));
        if graph.input_images_linear.contains(identifier) {
            let _ = write!(out, "<option><name v=\"{LINEAR_PREFIX}{}\"/><value v=\"1\"/></option>", xml_escape(identifier));
        }
    }
}

pub(super) fn read_resource(graph: &mut Graph, identifier: &str, resource: Option<String>) {
    // A matching preview option retains the interpretation explicitly chosen for this resource.
    if resource.is_some() && graph.input_images.get(identifier) == resource.as_ref() {
        return;
    }
    graph.input_images_linear.remove(identifier);
    if let Some(name) = resource {
        graph.input_images.insert(identifier.to_string(), name);
        graph.input_images_from_file.insert(identifier.to_string());
    } else {
        graph.input_images.remove(identifier);
        graph.input_images_from_file.remove(identifier);
    }
}

pub(super) fn read(option: XmlNode, graph: &mut Graph) {
    let Some(name) = child_value(option, "name") else { return };
    let Some(value) = child_value(option, "value") else { return };

    if let Some(identifier) = name.strip_prefix(IMAGE_PREFIX) {
        graph.input_images.insert(identifier.to_string(), value.to_string());
    } else if let Some(identifier) = name.strip_prefix(LINEAR_PREFIX)
        && value == "1"
    {
        graph.input_images_linear.insert(identifier.to_string());
    }
}

#[cfg(test)]
mod tests {
    use crate::sbs::{read_sbs, write_sbs};
    use materialcraft_core::{Graph, Package, Resource, ResourceBytes, Value};

    #[test]
    fn image_bridges_write_the_bound_package_resource() {
        let mut package = Package::new("Input images");
        let mut resource = Resource::new("grain", "grain.png", ResourceBytes::default());
        resource.folder = "Inputs/Maps".into();
        resource.source = "grain.png".into();
        package.resources.push(resource);
        let mut graph = Graph::new("Paint");
        for (definition, identifier) in [("input_grayscale", "height"), ("input_color", "paint"), ("input_color", "open")] {
            let id = graph.add_node(definition, [0.0; 2]);
            graph.node_mut(id).unwrap().params.insert("identifier".into(), Value::Text(identifier.into()));
        }
        for identifier in ["height", "paint"] {
            graph.input_images.insert(identifier.into(), "grain".into());
        }
        package.graphs.push(graph);

        let bytes = write_sbs(&package).unwrap();
        let doc = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let own = doc.descendants().find(|node| node.has_tag_name("dependency") && super::child_value(*node, "filename") == Some("?himself")).unwrap();
        let url = format!("pkg:///Inputs/Maps/grain?dependency={}", super::child_value(own, "uid").unwrap());
        let bindings: Vec<_> = doc
            .descendants()
            .filter(|node| node.has_tag_name("compInputBridge"))
            .map(|bridge| {
                bridge
                    .descendants()
                    .find(|p| p.has_tag_name("parameter") && super::child_value(*p, "name") == Some("bitmapresourcepath"))
                    .and_then(|p| p.descendants().find(|v| v.has_tag_name("constantValueString")))
                    .and_then(|v| v.attribute("v"))
            })
            .collect();
        assert_eq!(bindings, [Some(url.as_str()), Some(url.as_str()), None]);
        let reopened = read_sbs(&bytes, "input.sbs", &|_| None).unwrap().pkg;
        assert_eq!(reopened.graphs[0].input_images, package.graphs[0].input_images);
        assert!(reopened.graphs[0].input_images_from_file.is_empty());
    }

    #[test]
    fn image_bridge_binding_takes_precedence_over_a_legacy_preview_name() {
        let xml = br#"<package><dependencies><dependency><filename v="?himself"/><uid v="1"/></dependency></dependencies>
            <content><group><identifier v="Masks"/><content><resource><identifier v="grain"/><type v="bitmap"/>
            <format v="png"/><filepath v="grain.png"/></resource></content></group>
            <graph><identifier v="Paint"/><paraminputs><paraminput><identifier v="height"/><uid v="12"/><type v="2"/></paraminput></paraminputs>
            <options><option><name v="materialcraft.previewImage.height"/><value v="old"/></option></options>
            <compNodes><compNode><uid v="13"/><compImplementation><compInputBridge><entry v="12"/><parameters>
            <parameter><name v="bitmapresourcepath"/><relativeTo v="0"/><paramValue><constantValueString v="pkg:///Masks/grain?dependency=1"/>
            </paramValue></parameter></parameters></compInputBridge></compImplementation></compNode></compNodes></graph></content></package>"#;
        let package = read_sbs(xml, "paint.sbs", &|_| None).unwrap().pkg;
        assert_eq!(package.graphs[0].input_images.get("height").map(String::as_str), Some("grain"));
        assert!(package.graphs[0].input_images_from_file.contains("height"));
        let reopened = read_sbs(&write_sbs(&package).unwrap(), "again.sbs", &|_| None).unwrap().pkg;
        assert_eq!(package.graphs[0].input_images_from_file, reopened.graphs[0].input_images_from_file);
    }

    #[test]
    fn clearing_the_bridge_resource_clears_its_preview_interpretation() {
        let mut graph = Graph::new("Paint");
        graph.input_images.insert("height".into(), "grain".into());
        graph.input_images_linear.insert("height".into());
        super::read_resource(&mut graph, "height", Some("grain".into()));
        assert!(graph.input_images_linear.contains("height"));
        super::read_resource(&mut graph, "height", Some("different".into()));
        assert!(graph.input_images_linear.is_empty());
        assert!(graph.input_images_from_file.contains("height"));
        super::read_resource(&mut graph, "height", None);
        assert!(graph.input_images.is_empty());
        assert!(graph.input_images_from_file.is_empty());
    }
}
