use engine::{voxels::chunk::Chunk, world::World};

use crate::rendering::world_renderer::ChunkRenderState;

pub type RenderChunk = Chunk<ChunkRenderState>;
pub type RenderWorld = World<ChunkRenderState>;
