use std::mem::offset_of;

use bytemuck::{Pod, Zeroable};
use glam::{U8Vec4, UVec4};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GpuChunk {
    // World position of the chunk (x, y, z, min_y/max_y packed in w)
    pub position_and_y_range: UVec4,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ChunkVertex {
    // Position in chunk space
    // x, y, z, face_id (3 bits), 5 bits unused
    pub position: U8Vec4,
    // Index into global material buffer
    pub material_index: u16,
    pub _padding: u16,
}

impl ChunkVertex {
    pub fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: offset_of!(ChunkVertex, position) as u64,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Uint8x4,
                },
                // material_index + _padding
                wgpu::VertexAttribute {
                    offset: offset_of!(ChunkVertex, material_index) as u64,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

pub struct ChunkMeshData {
    pub position_and_y_range: UVec4,
    pub vertices: Vec<ChunkVertex>,
    pub indices: Vec<u32>,
}

impl ChunkMeshData {
    pub fn new() -> Self {
        ChunkMeshData {
            position_and_y_range: UVec4::ZERO,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn create(self, device: &wgpu::Device) -> ChunkMesh {
        let name = format!(
            "ChunkMesh_{}_{}_{}",
            self.position_and_y_range.x, self.position_and_y_range.y, self.position_and_y_range.z,
        );

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Vertex Buffer", name)),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Index Buffer", name)),
            contents: bytemuck::cast_slice(&self.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        ChunkMesh {
            position_and_y_range: self.position_and_y_range,
            vertex_buffer,
            index_buffer,
            index_count: self.indices.len() as u32,
        }
    }
}

pub struct ChunkMesh {
    pub position_and_y_range: UVec4,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}
