use craft_texture::Image;
use materialcraft_core::Value;
use std::collections::BTreeMap;

/// Inputs shared by one function evaluation
pub struct EvaluationContext<'a> {
    pub scope: super::FunctionScope,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub images: &'a [Option<Image>],
    /// Stored formats of image slots supplied by the caller.
    pub image_formats: &'a [materialcraft_core::OutputFormat],
    pub tile: bool,
    pub variables: &'a BTreeMap<String, Value>,
}

impl EvaluationContext<'_> {
    pub(super) fn variable(&self, name: &str, local: &BTreeMap<String, Value>) -> Result<Option<Value>, String> {
        if let Some(variable) = super::SystemVariable::named(name) {
            // A local name must never make a system variable available outside its caller's scope
            return variable.resolve(self);
        }

        Ok(local.get(name).or_else(|| self.variables.get(name)).cloned())
    }
}
