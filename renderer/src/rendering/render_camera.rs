use bitfield_struct::bitfield;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, UVec4, Vec3, Vec4};

use engine::{camera::Camera, game_loop::GameLoopTime};

use crate::{
    interpolated::Interpolated,
    rendering::{
        memory::typed_buffer::GpuBuffer,
        resolution::{PhysicalSizeExt, Resolution},
    },
};

#[bitfield(u32)]
struct CameraDebugFlags {
    #[bits(1)]
    pub enable_ao: bool,
    #[bits(1)]
    pub show_face_colors: bool,
    #[bits(30)]
    pub _reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct CameraUniform {
    view_projection_matrix: Mat4,
    view_projection_inverse_matrix: Mat4,
    // w is unused in both vectors, included for alignment
    camera_position: Vec4,
    sun_direction: Vec4,
    debug_flags: UVec4,
}

pub struct RenderCamera {
    camera: Interpolated<Camera>,
    pub interpolated_camera: Camera,
    pub sun_direction: Vec3,
    pub enable_ao: bool,
    pub show_face_colors: bool,
    resolution: Resolution,
    pub uniform_buffer: GpuBuffer<CameraUniform>,
}

impl RenderCamera {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, resolution: Resolution) -> Self {
        let camera = Camera::default();

        let sun_direction = Quat::from_rotation_x(-0.5)
            .mul_vec3(Vec3::new(0.0, 1.0, 0.0))
            .normalize();

        let camera_uniform = CameraUniform {
            view_projection_matrix: camera.view_projection_matrix,
            view_projection_inverse_matrix: camera.view_projection_inverse_matrix,
            camera_position: camera.eye.extend(0.0),
            sun_direction: sun_direction.extend(1.0),
            debug_flags: UVec4::new(CameraDebugFlags::new().with_enable_ao(true).into(), 0, 0, 0),
        };

        let uniform_buffer = GpuBuffer::new_with_data(
            device,
            queue,
            "Camera uniform buffer",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            &camera_uniform,
        );

        Self {
            camera: Interpolated::new(camera.clone()),
            interpolated_camera: camera,
            resolution,
            uniform_buffer,
            sun_direction: Quat::from_rotation_x(-0.5)
                .mul_vec3(Vec3::new(0.0, 1.0, 0.0))
                .normalize(),
            enable_ao: true,
            show_face_colors: false,
        }
    }

    /// Sets both previous and current camera to the same value (no interpolation).
    /// Use when teleporting or initializing.
    pub fn set_camera_immediate(&mut self, camera: &Camera) {
        self.camera.set_immediate(camera.clone());
        self.interpolated_camera = camera.clone();
        // TODO: This is probably not necessary, assuming the camera we receive already has up-to-date matrices.
        self.interpolated_camera
            .update_matrices(self.resolution.to_vec2());
        self.update_uniform_buffer();
    }

    /// Updates the camera for interpolation.
    /// Internally shifts the current state to previous before setting the new current.
    pub fn set_camera(&mut self, camera: &Camera, time: &GameLoopTime) {
        self.camera.set(camera.clone());
        self.interpolated_camera = self.camera.get(time.blending_factor);
        self.interpolated_camera
            .update_matrices(self.resolution.to_vec2());
        self.update_uniform_buffer();
    }

    pub fn eye(&self, blending_factor: f32) -> Vec3 {
        self.camera
            .previous
            .eye
            .lerp(self.camera.current.eye, blending_factor)
    }

    pub fn resize(&mut self, resolution: Resolution) {
        self.resolution = resolution;
    }

    pub fn view_projection(&self) -> &Mat4 {
        &self.camera.current.view_projection_matrix
    }

    pub fn inverse_view_projection(&self) -> &Mat4 {
        &self.camera.current.view_projection_inverse_matrix
    }

    fn update_uniform_buffer(&self) {
        self.uniform_buffer.write_data(&CameraUniform {
            view_projection_matrix: self.interpolated_camera.view_projection_matrix,
            view_projection_inverse_matrix: self.interpolated_camera.view_projection_inverse_matrix,
            camera_position: self.interpolated_camera.eye.extend(0.0),
            sun_direction: self.sun_direction.extend(0.0),
            debug_flags: UVec4::new(
                CameraDebugFlags::new()
                    .with_enable_ao(self.enable_ao)
                    .with_show_face_colors(self.show_face_colors)
                    .into(),
                0,
                0,
                0,
            ),
        });
    }

    pub fn toggle_ao(&mut self) {
        self.enable_ao = !self.enable_ao;
    }

    pub fn toggle_face_colors(&mut self) {
        self.show_face_colors = !self.show_face_colors;
    }
}
