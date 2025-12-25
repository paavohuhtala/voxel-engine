use std::{marker::PhantomData, mem::size_of};

use bytemuck::Pod;

/// Strongly typed GPU buffer wrapper.
#[derive(Debug, Clone)]
pub struct GpuBuffer<T> {
    inner: wgpu::Buffer,
    _marker: PhantomData<T>,
}

impl<T> GpuBuffer<T> {
    pub fn from_buffer(buffer: wgpu::Buffer) -> Self {
        GpuBuffer {
            inner: buffer,
            _marker: PhantomData,
        }
    }

    pub fn inner(&self) -> &wgpu::Buffer {
        &self.inner
    }
}

impl<T: Pod> GpuBuffer<T> {
    pub fn write_data(&self, queue: &wgpu::Queue, data: &T) {
        let byte_data: &[u8] = bytemuck::bytes_of(data);
        queue.write_buffer(&self.inner, 0, byte_data);
    }
}

/// Strongly typed GPU buffer wrapper for a fixed-size array.
#[derive(Debug, Clone)]
pub struct GpuBufferArray<T> {
    inner: wgpu::Buffer,
    _marker: PhantomData<T>,
}

impl<T> GpuBufferArray<T> {
    pub fn from_buffer(buffer: wgpu::Buffer) -> Self {
        GpuBufferArray {
            inner: buffer,
            _marker: PhantomData,
        }
    }

    pub fn inner(&self) -> &wgpu::Buffer {
        &self.inner
    }

    pub fn size(&self) -> u64 {
        self.inner.size()
    }

    pub fn capacity(&self) -> u64 {
        self.inner.size() / size_of::<T>() as u64
    }
}

impl<T: Pod> GpuBufferArray<T> {
    pub fn write_data(&self, queue: &wgpu::Queue, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        queue.write_buffer(&self.inner, 0, byte_data);
    }
}
