use std::mem::offset_of;

use bytemuck::{Pod, Zeroable};
use glam::{IVec4, U8Vec3, U8Vec4};
use wgpu::{VertexAttribute, VertexBufferLayout};

use engine::{
    math::aabb::{AABB8, PackedAABB},
    voxels::coord::ChunkPos,
};

use crate::rendering::memory::gpu_heap::GpuHeapHandle;

#[repr(C, align(16))]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GpuChunk {
    // World position of the chunk (x, y, z, w unused)
    pub position: IVec4,
    pub mesh_data_index_offset: u32,
    pub mesh_data_index_count: u32,
    pub mesh_data_vertex_offset: i32,
    pub aabb: PackedAABB,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ChunkVertex {
    // Position in chunk space
    // x, y, z, face_id (3 bits), 5 bits unused
    pub position: U8Vec4,
    // Index into global texture buffer
    pub texture_index: u16,
    // 2 bits are used for ambient occlusion (0-3), remaining 14 bits unused
    pub ambient_occlusion: u16,
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
                offset: offset_of!(ChunkVertex, texture_index) as u64,
                shader_location: 1,
                format: wgpu::VertexFormat::Uint16x2,
            },
        ],
    };
}

#[derive(Clone, Default)]
pub struct ChunkMeshData {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub vertices: Vec<ChunkVertex>,
    pub indices: Vec<u16>,
}

impl ChunkMeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_position(position: ChunkPos) -> Self {
        ChunkMeshData {
            position,
            aabb: AABB8::new(U8Vec3::splat(0), U8Vec3::splat(15)),
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }
}

pub struct ChunkMesh {
    pub position: ChunkPos,
    pub aabb: AABB8,
    pub vertices_handle: GpuHeapHandle<ChunkVertex>,
    pub indices_handle: GpuHeapHandle<u16>,
    pub index_count: u32,
}
