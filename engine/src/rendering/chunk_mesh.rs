use std::mem::offset_of;

use bytemuck::{Pod, Zeroable};
use glam::{IVec4, U8Vec4};
use wgpu::{VertexAttribute, VertexBufferLayout};

use crate::rendering::memory::gpu_heap::GpuHeapHandle;

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GpuChunk {
    // World position of the chunk (x, y, z, min_y/max_y packed in w)
    pub position_and_y_range: IVec4,
    pub mesh_data_index_offset: u32,
    pub mesh_data_index_count: u32,
    pub mesh_data_vertex_offset: i32,
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
    pub const VBL: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: size_of::<ChunkVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: offset_of!(ChunkVertex, position) as u64,
                shader_location: 0,
                format: wgpu::VertexFormat::Uint8x4,
            },
            VertexAttribute {
                offset: offset_of!(ChunkVertex, material_index) as u64,
                shader_location: 1,
                format: wgpu::VertexFormat::Uint32,
            },
        ],
    };
}

#[derive(Clone, Default)]
pub struct ChunkMeshData {
    // World position of the chunk (x, y, z, min_y/max_y packed in w)
    pub position_and_y_range: IVec4,
    pub vertices: Vec<ChunkVertex>,
    pub indices: Vec<u16>,
}

impl ChunkMeshData {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct ChunkMesh {
    pub position_and_y_range: IVec4,
    pub vertices_handle: GpuHeapHandle<ChunkVertex>,
    pub indices_handle: GpuHeapHandle<u16>,
    pub index_count: u32,
}
