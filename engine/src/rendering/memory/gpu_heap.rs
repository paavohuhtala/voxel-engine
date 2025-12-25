use std::{
    marker::PhantomData,
    sync::{Arc, RwLock},
};

use bytemuck::Pod;

use crate::rendering::memory::buddy_allocator::{AllocatorConfig, BuddyAllocator};

pub struct GpuHeapHandle<T> {
    offset: u64,
    size: u64,
    #[allow(unused)]
    count: u64,
    allocator: Arc<GpuHeap<T>>,
}

impl<T: Pod> GpuHeapHandle<T> {
    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn write_data(&self, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= self.size);
        self.allocator.write_data(self, bytemuck::cast_slice(data));
    }
}

impl<T> Drop for GpuHeapHandle<T> {
    fn drop(&mut self) {
        self.allocator.allocator.write().unwrap().free(self.offset);
    }
}

/// A managed GPU buffer with an internal Buddy allocator. Supports variable size allocations,
/// but with some fragmentation overhead.
pub struct GpuHeap<T> {
    buffer: wgpu::Buffer,
    queue: wgpu::Queue,
    allocator: RwLock<BuddyAllocator>,
    _marker: PhantomData<T>,
}

impl<T> GpuHeap<T> {
    pub fn new(
        device: &wgpu::Device,
        queue: wgpu::Queue,
        usage: wgpu::BufferUsages,
        size_bytes: u64,
        alignment_bytes: u64,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: size_bytes,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let allocator_config: AllocatorConfig = AllocatorConfig {
            total_size: size_bytes,
            min_order: 4, // 16 bytes
            alignment: alignment_bytes,
        };

        let allocator = BuddyAllocator::new(allocator_config);

        GpuHeap {
            buffer,
            queue,
            allocator: RwLock::new(allocator),
            _marker: PhantomData,
        }
    }

    pub fn allocate(self: Arc<Self>, count: u64) -> Option<GpuHeapHandle<T>> {
        let mut allocator = self.allocator.write().unwrap();
        if let Some(offset) = allocator.allocate(count) {
            Some(GpuHeapHandle {
                offset,
                count,
                size: count * size_of::<T>() as u64,
                allocator: self.clone(),
            })
        } else {
            None
        }
    }

    pub fn reallocate(&self, allocation: &mut GpuHeapHandle<T>, new_count: u64) -> Option<()> {
        let mut allocator = self.allocator.write().unwrap();
        let new_offset =
            allocator.reallocate(allocation.offset, new_count * size_of::<T>() as u64)?;
        allocation.offset = new_offset;
        allocation.count = new_count;
        allocation.size = new_count * size_of::<T>() as u64;
        Some(())
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn size(&self) -> u64 {
        self.allocator.read().unwrap().size()
    }
}

impl<T: Pod> GpuHeap<T> {
    pub fn write_data(&self, allocation: &GpuHeapHandle<T>, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= allocation.size);
        self.queue
            .write_buffer(&self.buffer, allocation.offset, bytemuck::cast_slice(data));
    }
}
