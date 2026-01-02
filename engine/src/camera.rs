use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}

// These impls could be moved to renderer, but in theory we might want to build game logic
// that depends on a player's view as well.
impl Camera {
    pub fn get_projection_matrix(&self, resolution: Vec2) -> Mat4 {
        Mat4::perspective_infinite_reverse_lh(45.0, resolution.x / resolution.y, 0.1)
    }

    pub fn get_view_matrix(&self) -> Mat4 {
        Mat4::look_at_lh(self.eye, self.target, self.up)
    }
}
