use serde::{Deserialize, Serialize};

/// A light at a point that orbits the scene's origin
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointLight {
    pub enabled: bool,
    /// Linear RGB
    pub color: [f32; 3],
    pub intensity: f32,
    /// Degrees around the vertical axis and above the horizon
    pub yaw: f32,
    pub pitch: f32,
    /// From the origin, in mesh radii
    pub distance: f32,
}

impl PointLight {
    pub const MAX_INTENSITY: f32 = 10.0;
    pub const DISTANCE: std::ops::RangeInclusive<f32> = 1.1..=20.0;

    /// The first light: white, from the camera's upper left
    pub fn first() -> PointLight {
        PointLight { enabled: false, color: [1.0; 3], intensity: 1.0, yaw: 335.0, pitch: 40.0, distance: 3.0 }
    }

    /// The second light: a dim gray fill from the other side
    pub fn second() -> PointLight {
        PointLight { color: [0.033; 3], yaw: 140.0, pitch: 20.0, ..PointLight::first() }
    }

    /// This light with every number finite and in range
    pub fn sanitized(self) -> PointLight {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };

        PointLight {
            enabled: self.enabled,
            color: self.color.map(|channel| finite(channel, 1.0).clamp(0.0, 1.0)),
            intensity: finite(self.intensity, 1.0).clamp(0.0, PointLight::MAX_INTENSITY),
            yaw: finite(self.yaw, 0.0).rem_euclid(360.0),
            pitch: finite(self.pitch, 0.0).clamp(-89.0, 89.0),
            distance: finite(self.distance, 3.0).clamp(*PointLight::DISTANCE.start(), *PointLight::DISTANCE.end()),
        }
    }
}

impl Default for PointLight {
    fn default() -> Self {
        PointLight::first()
    }
}
