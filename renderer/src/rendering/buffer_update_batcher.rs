use std::iter;

use bytemuck::Pod;
use wgpu::{Buffer, Device, util::DeviceExt};

use crate::rendering::memory::gpu_heap::GpuHeapHandle;

struct BatchedUpdate {
    buffer: Buffer,
    scratch_offset: usize,
    buffer_offset: u64,
    length: usize,
}

pub struct BufferUpdateBatcher {
    device: Device,
    data: Vec<u8>,
    updates: Vec<BatchedUpdate>,
}

impl BufferUpdateBatcher {
    pub fn new(device: Device, initial_capacity_bytes: usize) -> Self {
        BufferUpdateBatcher {
            device,
            data: Vec::with_capacity(initial_capacity_bytes),
            updates: Vec::new(),
        }
    }

    pub fn add_update<T: Pod>(&mut self, buffer: &Buffer, gpu_offset: u64, data: T) {
        let cpu_offset = self.data.len();
        let byte_data: &[u8] = bytemuck::bytes_of(&data);
        self.data.extend_from_slice(byte_data);

        self.updates.push(BatchedUpdate {
            buffer: buffer.clone(),
            scratch_offset: cpu_offset,
            buffer_offset: gpu_offset,
            length: byte_data.len(),
        });
    }

    pub fn add_heap_update<T>(&mut self, handle: &GpuHeapHandle<T>, data: &[T])
    where
        T: Pod,
    {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        if byte_data.is_empty() {
            return;
        }

        // Ensure scratch offset is 4-byte aligned (required by copy_buffer_to_buffer)
        let padding_needed = (4 - (self.data.len() % 4)) % 4;
        self.data.extend(iter::repeat_n(0u8, padding_needed));

        let cpu_offset = self.data.len();
        self.data.extend_from_slice(byte_data);

        // Round up copy length to 4-byte alignment
        let aligned_length = (byte_data.len() + 3) & !3;
        let extra_padding = aligned_length - byte_data.len();
        self.data.extend(iter::repeat_n(0u8, extra_padding));

        self.updates.push(BatchedUpdate {
            buffer: handle.buffer().clone(),
            scratch_offset: cpu_offset,
            buffer_offset: handle.byte_offset() as u64,
            length: aligned_length,
        });
    }

    pub fn flush(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.updates.is_empty() {
            return;
        }

        let staging_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BufferUpdateBatcher staging buffer"),
                contents: &self.data,
                usage: wgpu::BufferUsages::COPY_SRC,
            });

        for update in self.updates.drain(..) {
            encoder.copy_buffer_to_buffer(
                &staging_buffer,
                update.scratch_offset as u64,
                &update.buffer,
                update.buffer_offset,
                update.length as u64,
            );
        }

        self.data.clear();
    }
}
