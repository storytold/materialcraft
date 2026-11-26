/// A direction the 3D view's camera looks at the mesh from
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewpoint {
    pub id: &'static str,
    pub label: &'static str,
    /// Degrees around the vertical axis and above the horizon
    pub yaw: f32,
    pub pitch: f32,
}

impl Viewpoint {
    /// Every viewpoint, in menu order. The first is where a new view starts
    pub const ALL: [Viewpoint; 7] = [
        Viewpoint { id: "default", label: "Default", yaw: 35.0, pitch: 25.0 },
        Viewpoint { id: "front", label: "Front", yaw: 0.0, pitch: 0.0 },
        Viewpoint { id: "right", label: "Right", yaw: 90.0, pitch: 0.0 },
        Viewpoint { id: "back", label: "Back", yaw: 180.0, pitch: 0.0 },
        Viewpoint { id: "left", label: "Left", yaw: 270.0, pitch: 0.0 },
        Viewpoint { id: "top", label: "Top", yaw: 0.0, pitch: 89.0 },
        Viewpoint { id: "bottom", label: "Bottom", yaw: 0.0, pitch: -89.0 },
    ];

    pub fn of(id: &str) -> Option<Viewpoint> {
        Viewpoint::ALL.into_iter().find(|viewpoint| viewpoint.id == id)
    }
}
