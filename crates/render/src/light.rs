use materialcraft_geom::view::PointLight;

/// A point light as the renderer uses it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// From the scene center, in scene radii
    pub position: [f32; 3],
    /// Linear RGB times the intensity
    pub color: [f32; 3],
}

impl Light {
    /// The light of a view's point light, or `None` while it is switched off
    pub fn of(light: &PointLight) -> Option<Light> {
        let light = light.sanitized();
        if !light.enabled {
            return None;
        }

        let (yaw, pitch) = (light.yaw.to_radians(), light.pitch.to_radians());
        let direction = [pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()];
        Some(Light { position: direction.map(|axis| axis * light.distance), color: light.color.map(|channel| channel * light.intensity) })
    }
}
