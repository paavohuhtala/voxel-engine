use glam::{Mat4, Vec2, Vec3};

use crate::{
    math::frustum::Frustum,
    voxels::coord::{ChunkPos, WorldPosF},
};

#[derive(Debug, Clone)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,

    pub projection_matrix: Mat4,
    pub view_matrix: Mat4,
    pub view_projection_matrix: Mat4,
    pub view_projection_inverse_matrix: Mat4,
    pub frustum: Frustum,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 0.0),
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            projection_matrix: Mat4::IDENTITY,
            view_matrix: Mat4::IDENTITY,
            view_projection_matrix: Mat4::IDENTITY,
            view_projection_inverse_matrix: Mat4::IDENTITY,
            frustum: Frustum::default(),
        }
    }
}

// These impls could be moved to renderer, but in theory we might want to build game logic
// that depends on a player's view as well.
impl Camera {
    pub fn new(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self {
            eye,
            target,
            up,
            ..Default::default()
        }
    }

    pub fn update_matrices(&mut self, resolution: Vec2) {
        self.view_matrix = self.get_view_matrix();
        self.projection_matrix = self.get_projection_matrix(resolution);
        self.view_projection_matrix = self.projection_matrix * self.view_matrix;
        self.view_projection_inverse_matrix = self.view_projection_matrix.inverse();
        self.frustum = Frustum::from_inverse_view_projection(&self.view_projection_inverse_matrix);
    }

    pub fn get_current_chunk(&self) -> ChunkPos {
        WorldPosF(self.eye).to_chunk_pos()
    }

    fn get_projection_matrix(&self, resolution: Vec2) -> Mat4 {
        Mat4::perspective_infinite_reverse_lh(45.0, resolution.x / resolution.y, 0.1)
    }

    fn get_view_matrix(&self) -> Mat4 {
        Mat4::look_at_lh(self.eye, self.target, self.up)
    }
}
