use craft_texture::Image;
use materialcraft_core::{OutputFormat, Value};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum NodeOutput {
    Image(Image),
    /// A resized external texture whose automatic filtering can only sample its base level.
    BaseLevel(Image),
    /// A bound bitmap can report its bridge precision while filters sample its original integer texture.
    BoundBitmap {
        stored: Image,
        texture: Image,
        texture_format: OutputFormat,
    },
    Value(Value),
}

impl NodeOutput {
    pub fn as_image(&self) -> Option<&Image> {
        match self {
            Self::Image(image) | Self::BaseLevel(image) | Self::BoundBitmap { stored: image, .. } => Some(image),
            Self::Value(_) => None,
        }
    }

    pub fn as_value(&self) -> Option<&Value> {
        match self {
            Self::Image(_) | Self::BaseLevel(_) | Self::BoundBitmap { .. } => None,
            Self::Value(value) => Some(value),
        }
    }

    pub(crate) fn texture(&self, declared: OutputFormat) -> Option<(&Image, OutputFormat)> {
        match self {
            Self::BoundBitmap { texture, texture_format, .. } => Some((texture, *texture_format)),
            Self::Image(image) | Self::BaseLevel(image) => Some((image, declared)),
            Self::Value(_) => None,
        }
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        let bytes = |image: &Image| image.data().len().saturating_mul(size_of::<f32>());
        match self {
            Self::Image(image) | Self::BaseLevel(image) => bytes(image),
            Self::BoundBitmap { stored, texture, .. } if Arc::ptr_eq(stored.shared_data(), texture.shared_data()) => bytes(stored),
            Self::BoundBitmap { stored, texture, .. } => bytes(stored).saturating_add(bytes(texture)),
            Self::Value(Value::Text(text)) => size_of::<Value>().saturating_add(text.len()),
            Self::Value(_) => size_of::<Value>(),
        }
    }
}
