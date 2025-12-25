use std::mem::size_of;

use glam::Mat4;
use wgpu::{BufferDescriptor, BufferUsages};

pub struct RenderCommon {
    pub camera_uniform_buffer: wgpu::Buffer,
}

impl RenderCommon {
    pub fn new(device: &wgpu::Device) -> Self {
        let camera_uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Camera uniform buffer"),
            size: size_of::<Mat4>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            camera_uniform_buffer,
        }
    }
}
