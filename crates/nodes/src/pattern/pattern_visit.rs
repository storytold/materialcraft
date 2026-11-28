#[derive(Clone)]
pub(super) struct PatternVisit {
    pub node: u32,
    pub center: [f32; 2],
    pub extent: f32,
    pub depth: u32,
    pub number: u32,
    pub seed: u32,
}

impl PatternVisit {
    pub fn quadrant_centers(&self) -> [[f32; 2]; 4] {
        let half = self.extent * 0.5;
        let [left, top] = self.center.map(|v| v - self.extent * 0.25);
        // Moving between siblings rounds each step, including the return to the
        // left column when the preceding slot has no connected node.
        let right = left + half;
        let lower_left = right - half;
        let bottom = top + half;
        [[left, top], [right, top], [lower_left, bottom], [lower_left + half, bottom]]
    }
}
