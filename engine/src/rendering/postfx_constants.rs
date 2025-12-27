use bytemuck::{Pod, Zeroable};

use crate::rendering::memory::typed_buffer::GpuBuffer;

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct PostFxConstants {
    now: f32,
}

pub struct PostFxConstantsBuffer(GpuBuffer<PostFxConstants>);

impl PostFxConstantsBuffer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: std::mem::size_of::<PostFxConstants>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
            label: Some("Post FX constants buffer"),
        });

        let gpu_buffer = GpuBuffer::from_buffer(queue, buffer);
        gpu_buffer.write_data(&PostFxConstants { now: 0.0 });

        PostFxConstantsBuffer(gpu_buffer)
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.0.inner()
    }

    pub fn update(&self, now: f32) {
        let constants = PostFxConstants { now };
        self.0.write_data(&constants);
    }
}
