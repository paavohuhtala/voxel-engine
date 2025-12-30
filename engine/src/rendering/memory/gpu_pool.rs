use std::{
    marker::PhantomData,
    mem::size_of,
    sync::{Arc, RwLock},
};

use bytemuck::Pod;

use crate::memory::pool::Pool;

pub struct GpuPoolHandle<T> {
    index: u64,
    pool: Arc<GpuPool<T>>,
}

impl<T: Pod> GpuPoolHandle<T> {
    pub fn offset(&self) -> u64 {
        self.index
    }

    pub fn size(&self) -> u64 {
        size_of::<T>() as u64
    }

    pub fn write_data(&self, data: &T) {
        self.pool.write_data(self, data);
    }
}

impl<T> Drop for GpuPoolHandle<T> {
    fn drop(&mut self) {
        let mut pool = self.pool.pool.write().unwrap();
        pool.free(self.index);
    }
}

pub struct GpuPool<T> {
    buffer: wgpu::Buffer,
    queue: wgpu::Queue,
    pool: RwLock<Pool>,
    _marker: PhantomData<T>,
}

impl<T> GpuPool<T> {
    pub fn new(
        device: &wgpu::Device,
        queue: wgpu::Queue,
        usage: wgpu::BufferUsages,
        size_objects: u64,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: size_objects * size_of::<T>() as u64,
            usage,
            mapped_at_creation: false,
        });

        let pool = Pool::new(size_objects);

        GpuPool {
            buffer,
            queue,
            pool: RwLock::new(pool),
            _marker: PhantomData,
        }
    }

    pub fn allocate(self: &Arc<Self>) -> Option<GpuPoolHandle<T>> {
        let mut pool = self.pool.write().unwrap();
        let offset = pool.allocate()?;
        Some(GpuPoolHandle {
            index: offset,
            pool: Arc::clone(self),
        })
    }

    pub fn capacity(&self) -> u64 {
        self.pool.read().unwrap().capacity()
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

impl<T: Pod> GpuPool<T> {
    pub fn write_data(&self, allocation: &GpuPoolHandle<T>, data: &T) {
        let byte_offset = allocation.offset() * size_of::<T>() as u64;
        self.queue
            .write_buffer(&self.buffer, byte_offset, bytemuck::bytes_of(data));
    }
}
