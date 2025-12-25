use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}

impl Camera {
    pub fn get_vp_matrix(&self, resolution: Vec2) -> Mat4 {
        let view = Mat4::look_at_lh(self.eye, self.target, self.up);
        let projection = Mat4::perspective_lh(45.0, resolution.x / resolution.y, 0.1, 500.0);
        projection * view
    }
}
