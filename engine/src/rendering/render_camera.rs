use std::cell::{Cell, Ref, RefCell};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use wgpu::BufferDescriptor;

use crate::{
    camera::Camera,
    rendering::{memory::typed_buffer::GpuBuffer, resolution::Resolution},
};

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct CameraUniform {
    view_proj: Mat4,
    view_proj_inverse: Mat4,
    // w is unused in both vectors, included for alignment
    camera_position: Vec4,
    sun_direction: Vec4,
}

pub struct RenderCamera {
    pub camera: Camera,
    pub sun_direction: Vec3,
    pub enable_ao: bool,
    resolution: Resolution,
    view_proj: RefCell<Mat4>,
    is_dirty: Cell<bool>,
    should_update_uniform: Cell<bool>,
    pub uniform_buffer: GpuBuffer<CameraUniform>,
}

impl RenderCamera {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: Camera,
        resolution: Resolution,
    ) -> Self {
        let matrix =
            camera.get_vp_matrix(Vec2::new(resolution.width as f32, resolution.height as f32));

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            size: size_of::<CameraUniform>() as u64,
            mapped_at_creation: false,
            label: Some("Camera uniform buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_buffer = GpuBuffer::from_buffer(queue, uniform_buffer);
        uniform_buffer.write_data(&CameraUniform {
            view_proj: matrix,
            view_proj_inverse: matrix.inverse(),
            camera_position: Vec4::ZERO,
            sun_direction: Vec4::ZERO,
        });

        Self {
            camera,
            resolution,
            view_proj: RefCell::new(matrix),
            is_dirty: Cell::new(true),
            should_update_uniform: Cell::new(true),
            uniform_buffer,
            sun_direction: Quat::from_rotation_x(-0.5)
                .mul_vec3(Vec3::new(0.0, 1.0, 0.0))
                .normalize(),
            enable_ao: true,
        }
    }

    fn invalidate(&self) {
        self.is_dirty.set(true);
        self.should_update_uniform.set(true);
    }

    pub fn update_resolution(&mut self, resolution: Resolution) {
        if self.resolution != resolution {
            self.resolution = resolution;
            self.invalidate();
        }
    }

    pub fn update_camera(&mut self, camera: &Camera) {
        self.camera = camera.clone();
        self.invalidate();
    }

    fn update_view_proj(&self) {
        if !self.is_dirty.get() {
            return;
        }

        *self.view_proj.borrow_mut() = self.camera.get_vp_matrix(Vec2::new(
            self.resolution.width as f32,
            self.resolution.height as f32,
        ));
        self.is_dirty.set(false);
    }

    pub fn get_view_proj(&self) -> Ref<'_, Mat4> {
        self.update_view_proj();
        self.view_proj.borrow()
    }

    pub fn update_uniform_buffer(&self) {
        if !self.should_update_uniform.get() {
            return;
        }

        let view_proj = *self.get_view_proj();

        self.uniform_buffer.write_data(&CameraUniform {
            view_proj,
            // TODO: Cache inverse matrix
            view_proj_inverse: view_proj.inverse(),
            camera_position: self.camera.eye.extend(0.0),
            sun_direction: self
                .sun_direction
                .extend(if self.enable_ao { 1.0 } else { 0.0 }),
        });

        self.should_update_uniform.set(false);
    }

    pub fn toggle_ao(&mut self) {
        self.enable_ao = !self.enable_ao;
        self.invalidate();
    }
}
