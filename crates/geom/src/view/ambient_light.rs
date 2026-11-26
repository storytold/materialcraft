use serde::{Deserialize, Serialize};

/// Light that reaches every surface from everywhere
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AmbientLight {
    pub enabled: bool,
    /// Linear RGB
    pub color: [f32; 3],
}

impl Default for AmbientLight {
    fn default() -> Self {
        AmbientLight { enabled: true, color: [0.0; 3] }
    }
}
