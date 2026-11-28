use super::sbs_value::constant_xml;
use super::xml::xml_escape;
use materialcraft_core::{Node, Value};

pub(super) fn text_param(node: &Node, key: &str, default: &str) -> String {
    match node.params.get(key) {
        Some(Value::Text(s)) if !s.is_empty() => s.clone(),
        _ => default.to_string(),
    }
}
pub(super) fn param_xml(name: &str, value: &Value) -> Option<String> {
    Some(format!("<parameter><name v=\"{}\"/><relativeTo v=\"0\"/><paramValue>{}</paramValue></parameter>", xml_escape(name), constant_xml(value)?))
}

pub(super) fn resource_parameter_xml(parameter: &str, name: &str, self_uid: u64, package: &materialcraft_core::Package) -> String {
    let path = package.resource(name).map_or_else(|| format!("Resources/{name}"), |resource| resource.sbs_path());
    param_xml(parameter, &Value::Text(format!("pkg:///{path}?dependency={self_uid}"))).unwrap_or_default()
}
pub(super) fn special_filter_params_xml<'a>(
    filter: &str,
    value_of: &dyn Fn(&str) -> Option<&'a Value>,
    self_uid: u64,
    package: &materialcraft_core::Package,
) -> String {
    match filter {
        "bitmap" | "svg" => match value_of("resource") {
            Some(Value::Text(name)) if !name.is_empty() => {
                resource_parameter_xml(if filter == "svg" { "vectorresourcepath" } else { "bitmapresourcepath" }, name, self_uid, package)
            }
            _ => String::new(),
        },
        _ => String::new(),
    }
}
