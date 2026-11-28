use materialcraft_core::{BaseParameter, BaseParameters, Graph, OutputFormat, ParameterInheritance, Value};
use materialcraft_nodes::function::compile_with_literal_inputs;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedBaseParameters {
    pub size: [u32; 2],
    /// This texture inherits or evaluates a size controlled by a runtime parameter.
    #[serde(skip)]
    pub dynamic_size: bool,
    /// The resolved dimensions depend on a live graph input, including through size inheritance.
    #[serde(skip)]
    pub bound_size: bool,
    /// This node's seed expression reads a live graph input.
    #[serde(skip)]
    pub bound_seed: bool,
    /// A drawing property on this image or a connected source reads a live graph input.
    #[serde(skip)]
    pub bound_image: bool,
    /// Negative requested exponents on axes whose allocated texture is one texel.
    #[serde(skip)]
    pub size_underflow: [i32; 2],
    pub seed: u32,
    pub format: OutputFormat,
    pub tiling: [bool; 2],
    pub pixel_ratio: i32,
    pub pixel_size: [f32; 2],
}

// An image carries no random stream. Both relative modes add to the parent seed.
fn resolve_seed(mode: ParameterInheritance, offset: i32, parent: u32) -> u32 {
    mode.resolve(0, None, parent).wrapping_add(offset as u32)
}

impl ResolvedBaseParameters {
    pub(crate) fn graph(graph: &Graph, options: &crate::Options<'_>) -> Self {
        let parent = options.parent_base;
        let parent_ratio = options.parent_pixel_ratio.or_else(|| parent.map(|base| base.pixel_ratio));
        let (size, size_underflow) = graph.resolved_output_size(parent.map(|base| (base.size, base.size_underflow)), None);
        Self {
            size,
            dynamic_size: graph.absolute_output_size.is_none() && parent.is_some_and(|base| base.dynamic_size),
            bound_size: graph.output_size_setting().0 != materialcraft_core::SizeMode::Absolute && parent.is_some_and(|base| base.bound_size),
            bound_seed: false,
            bound_image: false,
            size_underflow,
            seed: resolve_seed(graph.seed_mode, graph.seed, parent.map_or(0, |base| base.seed)),
            format: match graph.format {
                OutputFormat::Inherit => parent.map_or(OutputFormat::Inherit, |base| base.format),
                explicit => explicit,
            },
            tiling: graph.tiling_mode.resolve(graph.tiling, options.parent_tiling, options.parent_tiling.unwrap_or(graph.tiling)),
            pixel_ratio: graph.pixel_ratio_mode.resolve(graph.pixel_ratio, parent_ratio, parent_ratio.unwrap_or(1)),
            pixel_size: parent.map_or([1.0; 2], |base| base.pixel_size),
        }
    }

    fn graph_function_value(
        self,
        parameter: BaseParameter,
        function: &materialcraft_core::FunctionGraph,
        options: &crate::Options<'_>,
        variables: &BTreeMap<String, Value>,
    ) -> Result<(Value, bool, bool), String> {
        let mut variables = variables.clone();
        self.variables(&mut variables);
        compile_with_literal_inputs(function, self.seed, &options.literal_inputs)
            .and_then(|program| {
                let dynamic = !program.output_is_literal() && !program.depends_only_on_image_size();
                let bound = program.reads_graph_parameter(&variables);
                let (value, _) = crate::parameter_function::evaluate(
                    &program,
                    parameter.value_type(),
                    &mut variables,
                    self.size,
                    crate::parameter_function::ParameterMode::for_base(parameter),
                    None,
                    None,
                )?;
                Ok((value, dynamic, bound))
            })
            .map_err(|error| format!("graph {}: {error}", parameter.label().to_ascii_lowercase()))
    }

    pub(crate) fn evaluate_graph_size(&mut self, graph: &Graph, options: &crate::Options<'_>, variables: &BTreeMap<String, Value>) -> Result<(), String> {
        let Some(function) = &graph.output_size_function else { return Ok(()) };
        let parent = options.parent_base.map(|base| (base.size, base.size_underflow));
        let (size, size_underflow) = if graph.output_size_setting().0 == materialcraft_core::SizeMode::Absolute {
            ([0; 2], [0; 2])
        } else {
            parent.unwrap_or_else(|| (graph.parent_size_px(), [0; 2]))
        };
        let context = Self { size, size_underflow, ..*self };
        let (value, dynamic, bound) = context.graph_function_value(BaseParameter::OutputSize, function, options, variables)?;
        let Value::Int2(offset) = value else { return Err("graph output size requires an integer pair".into()) };
        (self.size, self.size_underflow) = graph.resolved_output_size(parent, Some(offset));
        self.dynamic_size |= dynamic;
        self.bound_size |= bound;
        Ok(())
    }

    pub(crate) fn evaluate_graph_seed(&mut self, graph: &Graph, options: &crate::Options<'_>, variables: &BTreeMap<String, Value>) -> Result<(), String> {
        let Some(function) = &graph.seed_function else { return Ok(()) };
        let parent_seed = options.parent_base.map_or(0, |base| base.seed);
        // Graph seed functions see the allocated dimensions even when descendants retain a negative size exponent.
        let context = Self { seed: resolve_seed(graph.seed_mode, 0, parent_seed), size_underflow: [0; 2], ..*self };
        let (value, _, _) = context.graph_function_value(BaseParameter::RandomSeed, function, options, variables)?;
        let Value::Int(offset) = value else { return Err("graph random seed requires an integer".into()) };
        self.seed = resolve_seed(graph.seed_mode, offset, parent_seed);
        Ok(())
    }

    pub(crate) fn resolve(base: &BaseParameters, input: Option<Self>, parent: Self) -> Self {
        let dimensions = |base: Self| (base.size, base.size_underflow);
        let size_mode = BaseParameter::OutputSize.inheritance(base);
        let (size_base, underflow) = size_mode.resolve(([1; 2], [0; 2]), input.map(dimensions), dimensions(parent));
        let (size, size_underflow) = materialcraft_core::offset_dimensions(size_base, underflow, base.size);

        let inherited_pixel_size = base.pixel_size_mode.resolve([1.0; 2], input.map(|base| base.pixel_size), parent.pixel_size);
        Self {
            size,
            size_underflow,
            dynamic_size: size_mode.resolve(false, input.map(|base| base.dynamic_size), parent.dynamic_size),
            bound_size: size_mode.resolve(false, input.map(|base| base.bound_size), parent.bound_size),
            bound_seed: false,
            bound_image: false,
            seed: resolve_seed(base.seed_mode, base.seed, parent.seed),
            format: BaseParameter::Format.inheritance(base).resolve(base.format, input.map(|base| base.format), parent.format),
            tiling: base.tiling_mode.resolve(base.tiling, input.map(|base| base.tiling), parent.tiling),
            pixel_ratio: base.pixel_ratio_mode.resolve(base.pixel_ratio, input.map(|base| base.pixel_ratio), parent.pixel_ratio),
            pixel_size: std::array::from_fn(|axis| (inherited_pixel_size[axis] * base.pixel_size[axis]).clamp(-8192.0, 8192.0)),
        }
    }

    pub(crate) fn variables(self, variables: &mut BTreeMap<String, Value>) {
        self.size_variables(variables);
        variables.insert("$tiling".into(), Value::Int(i32::from(self.tiling[0]) | (i32::from(self.tiling[1]) << 1)));
        variables.insert("$pixelratio".into(), Value::Int(self.pixel_ratio));
    }

    fn size_variables(self, variables: &mut BTreeMap<String, Value>) {
        if self.size_underflow == [0; 2] {
            variables.remove("$sizelog2");
        } else {
            let exponents =
                std::array::from_fn(|axis| if self.size_underflow[axis] < 0 { self.size_underflow[axis] as f32 } else { (self.size[axis] as f32).log2() });
            variables.insert("$sizelog2".into(), Value::Float2(exponents));
        }
    }
}

pub(crate) fn evaluate(
    base: &BaseParameters,
    input: Option<ResolvedBaseParameters>,
    parent: ResolvedBaseParameters,
    variables: &mut std::borrow::Cow<'_, BTreeMap<String, Value>>,
    literal_inputs: &[String],
    mut assigned_integers: Option<&mut Vec<String>>,
) -> Result<(ResolvedBaseParameters, BTreeMap<BaseParameter, Value>), String> {
    if base.functions.is_empty() {
        return Ok((ResolvedBaseParameters::resolve(base, input, parent), BTreeMap::new()));
    }

    // Evaluation mutates literals while borrowing the attached function graphs
    let mut evaluated = BaseParameters {
        size_mode: base.size_mode,
        size: base.size,
        format: base.format,
        format_mode: base.format_mode,
        seed: base.seed,
        seed_mode: base.seed_mode,
        pixel_size: base.pixel_size,
        pixel_size_mode: base.pixel_size_mode,
        functions: BTreeMap::new(),
        tiling: base.tiling,
        tiling_mode: base.tiling_mode,
        pixel_ratio: base.pixel_ratio,
        pixel_ratio_mode: base.pixel_ratio_mode,
    };
    let mut values = BTreeMap::new();
    let mut computed_size = false;
    let mut bound_size = false;
    let mut bound_seed = false;
    let mut resolved = ResolvedBaseParameters::resolve(&evaluated, input, parent);

    // Size and seed establish the context used by the remaining parameter functions
    for parameter in [
        BaseParameter::RandomSeed,
        BaseParameter::OutputSize,
        BaseParameter::Format,
        BaseParameter::PixelSize,
        BaseParameter::PixelRatio,
        BaseParameter::Tiling,
    ] {
        if let Some(function) = base.functions.get(&parameter) {
            resolved.variables(variables.to_mut());
            let size = if parameter == BaseParameter::OutputSize && parameter.inheritance(base) != ParameterInheritance::Absolute {
                let inherited = parameter.inheritance(base).resolve(parent, input, parent);
                inherited.size_variables(variables.to_mut());
                inherited.size
            } else {
                resolved.size
            };

            let (value, _) = compile_with_literal_inputs(function, resolved.seed, literal_inputs)
                .and_then(|program| {
                    if parameter == BaseParameter::OutputSize {
                        computed_size = !program.output_is_literal() && !program.depends_only_on_image_size();
                        bound_size = program.reads_graph_parameter(variables);
                    } else if parameter == BaseParameter::RandomSeed {
                        bound_seed = program.reads_graph_parameter(variables);
                    }
                    crate::parameter_function::evaluate(
                        &program,
                        parameter.value_type(),
                        variables.to_mut(),
                        size,
                        crate::parameter_function::ParameterMode::for_base(parameter),
                        None,
                        assigned_integers.as_deref_mut(),
                    )
                })
                .map_err(|error| format!("function of `{}`: {error}", parameter.identifier()))?;
            parameter.set_value(&mut evaluated, &value)?;
            parameter.set_inheritance(&mut evaluated, parameter.inheritance(base));
            resolved = ResolvedBaseParameters::resolve(&evaluated, input, parent);
        }

        values.insert(parameter, parameter.value(&evaluated));
    }

    resolved.dynamic_size |= computed_size;
    resolved.bound_size |= bound_size;
    resolved.bound_seed = bound_seed;
    Ok((resolved, values))
}
