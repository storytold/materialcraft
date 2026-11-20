use super::xml::{child_element, child_value, children_named, format_float, xml_escape};
use materialcraft_core::{ExposedParameter, ParameterEditor, Value};
use roxmltree::Node;
use std::fmt::Write;

pub(super) fn read(element: Node<'_, '_>, input: &mut ExposedParameter) {
    input.description = child_element(element, "attributes").and_then(|attributes| child_value(attributes, "description")).unwrap_or("").into();
    let Some(widget) = child_element(element, "defaultWidget") else {
        return;
    };

    let name = child_value(widget, "name").unwrap_or("");
    let mut color = name == "color";
    for option in child_element(widget, "options").into_iter().flat_map(|options| children_named(options, "option")) {
        let text = child_value(option, "value").unwrap_or("");
        let number = text.split([' ', ';']).next().and_then(|value| value.parse::<f32>().ok()).filter(|value| value.is_finite());
        match child_value(option, "name").unwrap_or("") {
            "clamp" => input.clamp = number.is_some_and(|value| value != 0.0),
            "step" => input.step = number.filter(|step| *step > 0.0),
            "valueInterpretation" if text == "color" => color = true,
            "parameters" if name == "dropdownlist" => {
                // Labels are written unescaped, so a non-numeric fragment after a label belongs to it
                let mut parts = text.split(';').skip(1).peekable();
                while let (Some(index), Some(label)) = (parts.next(), parts.next()) {
                    // The model's enum values are contiguous; sparse external lists remain numeric
                    if index.parse::<usize>().ok() != Some(input.options.len()) {
                        input.options.clear();
                        break;
                    }
                    let mut label = label.to_string();
                    while let Some(rest) = parts.next_if(|part| part.parse::<usize>().is_err()) {
                        label.push(';');
                        label.push_str(rest);
                    }
                    input.options.push(label);
                }
            }
            _ => {}
        }
    }
    if name == "dropdownlist"
        && !input.options.is_empty()
        && let Some(value) = input.default.as_i32()
    {
        input.default = Value::Enum(value.max(0) as u32);
    }
    if color && matches!(input.default, Value::Float4(_)) {
        // Color defaults use the same displayed-to-linear conversion as node colors
        if let Some(value) = child_element(element, "defaultValue")
            .and_then(super::sbs_value::sbs_constant)
            .and_then(|value| super::sbs_value::to_param_value(&Value::Color([0.0; 4]), &value, 0))
        {
            input.default = value;
        }
    }

    let editor = match name {
        "slider" => Some(ParameterEditor::Slider),
        "inputfield" => Some(ParameterEditor::Input),
        "color" => Some(ParameterEditor::Color),
        "dropdownlist" => Some(ParameterEditor::Dropdown),
        "buttons" => Some(ParameterEditor::Toggle),
        "text" => Some(ParameterEditor::Text),
        _ => None,
    };
    input.editor = editor.filter(|editor| *editor != ParameterEditor::for_value(&input.default) && ParameterEditor::choices(&input.default).contains(editor));
}

pub(super) fn write(out: &mut String, input: &ExposedParameter) {
    let name = match input.editor() {
        ParameterEditor::Slider => "slider",
        ParameterEditor::Input => "inputfield",
        ParameterEditor::Color => "color",
        ParameterEditor::Dropdown => "dropdownlist",
        ParameterEditor::Toggle => "buttons",
        ParameterEditor::Text => "text",
    };
    let _ = write!(out, "<defaultWidget><name v=\"{name}\"/><options>");
    for (name, value) in [("min", format_float(input.min)), ("max", format_float(input.max)), ("clamp", u8::from(input.clamp).to_string())] {
        option(out, name, &value);
    }
    if matches!(input.default, Value::Color(_)) {
        option(out, "valueInterpretation", "color");
    }
    if let Some(step) = input.step {
        option(out, "step", &format_float(step));
    }
    if !input.options.is_empty() {
        let mut values = input.default.as_i32().unwrap_or(0).to_string();
        for (index, label) in input.options.iter().enumerate() {
            let _ = write!(values, ";{index};{label}");
        }
        option(out, "parameters", &values);
    }
    out.push_str("</options></defaultWidget>");
}

fn option(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, "<option><name v=\"{name}\"/><value v=\"{}\"/></option>", xml_escape(value));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(options: &[&str]) -> ExposedParameter {
        ExposedParameter {
            id: "mode".into(),
            label: "Mode".into(),
            group: String::new(),
            default: Value::Enum(0),
            description: String::new(),
            editor: Some(ParameterEditor::Dropdown),
            clamp: false,
            step: None,
            min: 0.0,
            max: 1.0,
            options: options.iter().map(|label| (*label).into()).collect(),
        }
    }

    fn read_value(value: &str) -> Vec<String> {
        let xml = format!(
            r#"<p><defaultWidget><name v="dropdownlist"/><options><option><name v="parameters"/><value v="{value}"/></option></options></defaultWidget></p>"#
        );
        let document = roxmltree::Document::parse(&xml).unwrap();
        let mut loaded = param(&[]);
        loaded.default = Value::Int(0);
        read(document.root_element(), &mut loaded);
        loaded.options
    }

    #[test]
    fn dropdown_labels_with_semicolons_round_trip() {
        let labels = ["Fast; low", "Slow", "a\\b", "end;", "x;y;z"];
        let input = param(&labels);
        let mut xml = String::from("<p>");
        write(&mut xml, &input);
        xml.push_str("</p>");
        let document = roxmltree::Document::parse(&xml).unwrap();
        let mut loaded = param(&[]);
        loaded.default = Value::Int(0);
        read(document.root_element(), &mut loaded);
        assert_eq!(loaded.options, labels);
    }

    #[test]
    fn plain_and_sparse_dropdown_parameters_read_as_before() {
        assert_eq!(read_value("0;0;A;1;B\\C"), ["A", "B\\C"]);
        assert!(read_value("0;0;A;5;B").is_empty());
    }
}
