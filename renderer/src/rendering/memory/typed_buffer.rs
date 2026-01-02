use std::{marker::PhantomData, mem::size_of};

use bytemuck::Pod;
use wgpu::util::DeviceExt;

/// Strongly typed GPU buffer wrapper.
#[derive(Debug, Clone)]
pub struct GpuBuffer<T> {
    inner: wgpu::Buffer,
    queue: wgpu::Queue,
    _marker: PhantomData<T>,
}

impl<T> GpuBuffer<T> {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        usages: wgpu::BufferUsages,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            mapped_at_creation: false,
            size: size_of::<T>() as u64,
            usage: usages,
        });

        GpuBuffer {
            inner: buffer,
            queue: queue.clone(),
            _marker: PhantomData,
        }
    }

    pub fn from_buffer(queue: &wgpu::Queue, buffer: wgpu::Buffer) -> Self {
        GpuBuffer {
            inner: buffer,
            queue: queue.clone(),
            _marker: PhantomData,
        }
    }

    pub fn inner(&self) -> &wgpu::Buffer {
        &self.inner
    }
}

impl<T: Pod> GpuBuffer<T> {
    pub fn write_data(&self, data: &T) {
        let byte_data: &[u8] = bytemuck::bytes_of(data);
        self.queue.write_buffer(&self.inner, 0, byte_data);
    }

    pub fn new_with_data(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        usages: wgpu::BufferUsages,
        data: &T,
    ) -> Self {
        let byte_data: &[u8] = bytemuck::bytes_of(data);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: byte_data,
            usage: usages,
        });

        GpuBuffer {
            inner: buffer,
            queue: queue.clone(),
            _marker: PhantomData,
        }
    }
}

/// Strongly typed GPU buffer wrapper for a fixed-size array.
#[derive(Debug, Clone)]
pub struct GpuBufferArray<T> {
    inner: wgpu::Buffer,
    queue: wgpu::Queue,
    _marker: PhantomData<T>,
}

impl<T> GpuBufferArray<T> {
    pub fn from_buffer(queue: &wgpu::Queue, buffer: wgpu::Buffer) -> Self {
        GpuBufferArray {
            inner: buffer,
            queue: queue.clone(),
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
    pub fn write_data(&self, data: &[T]) {
        let byte_data: &[u8] = bytemuck::cast_slice(data);
        self.queue.write_buffer(&self.inner, 0, byte_data);
    }
}
