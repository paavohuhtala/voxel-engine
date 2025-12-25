use std::cell::{Cell, Ref, RefCell};

use glam::{Mat4, Vec2};
use wgpu::util::DeviceExt;

use crate::{camera::Camera, rendering::resolution::Resolution};

pub struct RenderCamera {
    camera: Camera,
    resolution: Resolution,
    view_proj: RefCell<Mat4>,
    is_dirty: Cell<bool>,
    should_update_uniform: Cell<bool>,
    pub uniform_buffer: wgpu::Buffer,
}

impl RenderCamera {
    pub fn new(device: &wgpu::Device, camera: Camera, resolution: Resolution) -> Self {
        let matrix =
            camera.get_vp_matrix(Vec2::new(resolution.width as f32, resolution.height as f32));

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera uniform buffer"),
            contents: bytemuck::cast_slice(&[matrix]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            camera,
            resolution,
            view_proj: RefCell::new(matrix),
            is_dirty: Cell::new(true),
            should_update_uniform: Cell::new(true),
            uniform_buffer,
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

    pub fn update_uniform_buffer(&self, queue: &wgpu::Queue) {
        if !self.should_update_uniform.get() {
            return;
        }

        let view_proj = *self.get_view_proj();

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[view_proj]));

        self.should_update_uniform.set(false);
    }
}
