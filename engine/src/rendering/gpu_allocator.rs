use std::{
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use bytemuck::Pod;

use crate::rendering::allocator::{Allocator, AllocatorConfig};

pub struct GpuAllocation<T> {
    offset: u64,
    size: u64,
    allocator: Arc<GpuAllocator<T>>,
}

impl<T: Pod> GpuAllocation<T> {
    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn write_data(&self, queue: &wgpu::Queue, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= self.size);
        self.allocator
            .write_data(queue, self, bytemuck::cast_slice(data));
    }
}

impl<T> Drop for GpuAllocation<T> {
    fn drop(&mut self) {
        self.allocator.allocator.write().unwrap().free(self.offset);
    }
}

pub struct GpuAllocator<T> {
    buffer: wgpu::Buffer,
    allocator: RwLock<Allocator>,
    _marker: PhantomData<T>,
}

impl<T> GpuAllocator<T> {
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, label: String, size: u64) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let allocator_config = AllocatorConfig {
            total_size: size,
            min_order: 4, // 16 bytes
            // TODO: Is align_of correct for GPU buffers?
            alignment: align_of::<T>() as u64,
        };

        let allocator = Allocator::new(allocator_config);

        GpuAllocator {
            buffer,
            allocator: RwLock::new(allocator),
            _marker: PhantomData,
        }
    }

    pub fn allocate(self: Arc<Self>, size: u64) -> Option<GpuAllocation<T>> {
        let mut allocator = self.allocator.write().unwrap();
        if let Some(offset) = allocator.allocate(size) {
            Some(GpuAllocation {
                offset,
                size,
                allocator: self.clone(),
            })
        } else {
            None
        }
    }

    pub fn reallocate(self: Arc<Self>, allocation: &mut GpuAllocation<T>, new_size: u64) {
        let mut allocator = self.allocator.write().unwrap();
        if let Some(new_offset) = allocator.reallocate(allocation.offset, new_size) {
            allocation.offset = new_offset;
            allocation.size = new_size;
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn size(&self) -> u64 {
        self.allocator.read().unwrap().size()
    }
}

impl<T: Pod> GpuAllocator<T> {
    pub fn write_data(&self, queue: &wgpu::Queue, allocation: &GpuAllocation<T>, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= allocation.size);
        queue.write_buffer(&self.buffer, allocation.offset, bytemuck::cast_slice(data));
    }
}
