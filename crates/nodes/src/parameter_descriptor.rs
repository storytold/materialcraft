use materialcraft_core::{BaseParameter, BaseParameters, Node, Package, Value, ValueType};

/// Parameter metadata shared by commands, Properties, and dynamic-value validation
#[derive(Clone, Debug)]
pub struct ParameterDescriptor {
    pub id: String,
    pub label: String,
    pub default: Value,
    pub min: f32,
    pub max: f32,
    pub options: Vec<String>,
    pub supports_function: bool,
}

impl ParameterDescriptor {
    pub fn base(parameter: BaseParameter) -> Self {
        let (min, max, options): (f32, f32, &[&str]) = match parameter {
            BaseParameter::OutputSize => (-12.0, 13.0, &[]),
            BaseParameter::Format => (0.0, 3.0, &["8 bits per channel", "16 bits per channel", "16 bit floating point", "32 bit floating point"]),
            BaseParameter::PixelSize => (0.001, 16.0, &[]),
            BaseParameter::PixelRatio => (0.0, 1.0, &["Stretch", "Square"]),
            BaseParameter::Tiling => (0.0, 3.0, &["No tiling", "Horizontal tiling", "Vertical tiling", "H and V tiling"]),
            BaseParameter::RandomSeed => (0.0, 1000.0, &[]),
        };

        Self {
            id: parameter.identifier().into(),
            label: parameter.label().into(),
            default: parameter.value(&BaseParameters::default()),
            min,
            max,
            options: options.iter().map(|option| (*option).into()).collect(),
            supports_function: true,
        }
    }

    pub fn for_node(package: &Package, node: &Node, parameter: &str) -> Option<Self> {
        if node.def == "grayscale_conversion" && parameter == "weights" {
            // A single function can drive the four channel controls together.
            return Some(Self {
                id: parameter.into(),
                label: "RGBA weights".into(),
                default: Value::Float4([1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0]),
                min: 0.0,
                max: 1.0,
                options: Vec::new(),
                supports_function: true,
            });
        }
        if node.def == "graph_instance" {
            let source = node
                .instance
                .as_ref()
                .filter(|instance| !instance.package.is_empty())
                .map_or(Some(package), |instance| package.dependencies.get(&instance.package).map(AsRef::as_ref));
            let name = node.instance.as_ref().map(|instance| instance.graph.as_str()).or_else(|| match node.params.get("graph") {
                Some(Value::Text(name)) => Some(name.as_str()),
                _ => None,
            });
            let graph = source.zip(name).and_then(|(source, name)| source.graph_by_name(name)).map(|(_, graph)| graph);
            if let Some(graph) = graph
                && let Some(input) = graph.exposed.iter().find(|input| input.id == parameter)
            {
                return Some(Self {
                    id: input.id.clone(),
                    label: input.label.clone(),
                    default: graph.input_value(parameter).unwrap_or(&input.default).clone(),
                    min: input.min,
                    max: input.max,
                    options: input.options.clone(),
                    supports_function: ValueType::of(&input.default).is_some(),
                });
            }

            let bridge = graph.and_then(|graph| {
                graph.nodes.iter().find(|bridge| {
                    bridge.def == "input_value"
                        && match bridge.params.get("identifier") {
                            Some(Value::Text(name)) if !name.is_empty() => name == parameter,
                            _ => parameter == "input",
                        }
                })
            });
            let default = if let Some(bridge) = bridge {
                bridge.params.get("default").cloned().unwrap_or(Value::Float(0.0))
            } else {
                let interface = node.instance.as_ref()?;
                interface.defaults.get(parameter).cloned().or_else(|| {
                    let port = interface.inputs.iter().find(|port| port.identifier == parameter)?;
                    Some(ValueType::from_code(port.type_code)?.default_value())
                })?
            };
            return Some(Self {
                id: parameter.into(),
                label: parameter.into(),
                supports_function: ValueType::of(&default).is_some(),
                default,
                min: 0.0,
                max: 1.0,
                options: Vec::new(),
            });
        }

        let definition = crate::find(&node.def)?.param(parameter)?;
        let default = if node.def == "input_value" && parameter == "default" {
            node.params.get(parameter).filter(|value| ValueType::of(value).is_some()).unwrap_or(&definition.default)
        } else {
            &definition.default
        };

        Some(Self {
            id: parameter.into(),
            label: definition.label.into(),
            default: default.clone(),
            min: definition.min,
            max: definition.max,
            options: definition.options.iter().map(|option| (*option).into()).collect(),
            supports_function: !definition.resource && ValueType::of(&definition.default).is_some() && !matches!(parameter, "identifier" | "graph"),
        })
    }
}
