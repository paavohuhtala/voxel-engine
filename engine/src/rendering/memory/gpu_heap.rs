use std::{
    marker::PhantomData,
    mem::size_of,
    sync::{Arc, RwLock},
};

use bytemuck::Pod;

use crate::rendering::memory::buddy_allocator::{AllocatorConfig, BuddyAllocator};

pub struct GpuHeapHandle<T> {
    byte_offset: u64,
    index: u64,
    size: u64,
    #[allow(unused)]
    count: u64,
    allocator: Arc<GpuHeap<T>>,
}

impl<T: Pod> GpuHeapHandle<T> {
    pub fn byte_offset(&self) -> u64 {
        self.byte_offset
    }

    pub fn index(&self) -> u64 {
        self.index
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
        self.allocator
            .allocator
            .write()
            .unwrap()
            .free(self.byte_offset);
    }
}

/// A managed GPU buffer with an internal Buddy allocator. Supports variable size allocations,
/// but with some fragmentation overhead.
pub struct GpuHeap<T> {
    buffer: wgpu::Buffer,
    queue: wgpu::Queue,
    allocator: RwLock<BuddyAllocator>,
    #[allow(unused)]
    label: String,
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
            label,
            _marker: PhantomData,
        }
    }

    pub fn allocate(self: Arc<Self>, count: u64) -> Option<GpuHeapHandle<T>> {
        let mut allocator = self.allocator.write().unwrap();
        let size_bytes = count * size_of::<T>() as u64;
        if let Some(byte_offset) = allocator.allocate(size_bytes) {
            Some(GpuHeapHandle {
                byte_offset,
                index: byte_offset / size_of::<T>() as u64,
                count,
                size: size_bytes,
                allocator: self.clone(),
            })
        } else {
            None
        }
    }

    pub fn reallocate(&self, allocation: &mut GpuHeapHandle<T>, new_count: u64) -> Option<()> {
        let new_size = new_count * size_of::<T>() as u64;
        let mut allocator = self.allocator.write().unwrap();
        let new_byte_offset = allocator.reallocate(allocation.byte_offset, new_size)?;
        allocation.byte_offset = new_byte_offset;
        allocation.index = new_byte_offset / size_of::<T>() as u64;
        allocation.count = new_count;
        allocation.size = new_size;
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
        self.queue.write_buffer(
            &self.buffer,
            allocation.byte_offset,
            bytemuck::cast_slice(data),
        );
    }
}
