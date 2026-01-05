use std::{
    marker::PhantomData,
    mem::size_of,
    sync::{Arc, RwLock},
};

use bytemuck::Pod;
use offset_allocator::{Allocation, Allocator, StorageReport};

use crate::rendering::buffer_update_batcher::BufferUpdateBatcher;

pub struct GpuHeapHandle<T> {
    pub size_bytes: u32,
    pub size_words: u32,
    allocation: Allocation,
    allocator: Arc<GpuHeap<T>>,
}

impl<T: Pod> GpuHeapHandle<T> {
    pub fn word_offset(&self) -> u32 {
        self.allocation.offset
    }

    pub fn byte_offset(&self) -> u32 {
        self.allocation.offset * 4
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        self.allocator.buffer()
    }

    pub fn write_data(&self, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        if byte_data.is_empty() {
            return;
        }
        self.allocator.write_data(self, bytemuck::cast_slice(data));
    }

    pub fn write_data_batched(&self, batcher: &mut BufferUpdateBatcher, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        if byte_data.is_empty() {
            return;
        }
        self.allocator.write_data_batched(batcher, self, data);
    }
}

impl<T> Drop for GpuHeapHandle<T> {
    fn drop(&mut self) {
        self.allocator
            .allocator
            .write()
            .unwrap()
            .free(self.allocation);
    }
}

/// A managed GPU buffer with an internal offset allocator.
/// Always uses 4-byte alignment.
pub struct GpuHeap<T> {
    buffer: wgpu::Buffer,
    queue: wgpu::Queue,
    #[allow(unused)]
    size_bytes: u32,
    allocator: RwLock<Allocator>,
    #[allow(unused)]
    label: String,
    _marker: PhantomData<T>,
}

impl<T> GpuHeap<T> {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        usage: wgpu::BufferUsages,
        size_bytes: u32,
        max_allocs: u32,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: size_bytes as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // To ensure 4-byte alignment, instead of using offset-allocator to manage bytes, we use 4 byte words (u32) instead.
        assert!(
            size_bytes.is_multiple_of(4),
            "GpuHeap size must be a multiple of 4 bytes"
        );
        let size_words = size_bytes / 4;
        let allocator = Allocator::with_max_allocs(size_words, max_allocs);

        GpuHeap {
            buffer,
            queue: queue.clone(),
            size_bytes,
            allocator: RwLock::new(allocator),
            label,
            _marker: PhantomData,
        }
    }

    pub fn alignment(&self) -> u32 {
        4
    }

    pub fn allocate(self: Arc<Self>, count: u32) -> Option<GpuHeapHandle<T>> {
        let mut allocator = self.allocator.write().unwrap();
        let size_bytes = count * size_of::<T>() as u32;
        let size_words = size_bytes.div_ceil(4); // Round up to nearest word
        if let Some(allocation) = allocator.allocate(size_words) {
            Some(GpuHeapHandle {
                size_bytes,
                size_words,
                allocation,
                allocator: self.clone(),
            })
        } else {
            let report = allocator.storage_report();
            log::error!(
                "ALLOCATION FAILED! Requested {} words ({} bytes), report: total_free_space={}, largest_free_region={}",
                size_words,
                size_bytes,
                report.total_free_space,
                report.largest_free_region
            );
            None
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn allocator(&self) -> &RwLock<Allocator> {
        &self.allocator
    }

    pub fn capacity_bytes(&self) -> u32 {
        self.size_bytes
    }

    pub fn storage_report(&self) -> StorageReport {
        let allocator = self.allocator.read().unwrap();
        allocator.storage_report()
    }
}

impl<T: Pod> GpuHeap<T> {
    pub fn write_data(&self, allocation: &GpuHeapHandle<T>, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= allocation.size_bytes as u64);
        self.queue.write_buffer(
            &self.buffer,
            allocation.byte_offset() as u64,
            bytemuck::cast_slice(data),
        );
    }

    pub fn write_data_batched(
        &self,
        batcher: &mut BufferUpdateBatcher,
        allocation: &GpuHeapHandle<T>,
        data: &[T],
    ) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        assert!(byte_data.len() as u64 <= allocation.size_bytes as u64);
        batcher.add_heap_update(allocation, data);
    }
}
