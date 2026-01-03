use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use engine::{
    assets::blocks::BlockDatabase,
    voxels::{chunk::Chunk, coord::ChunkPos},
    world::World,
};

use crate::rendering::{
    chunk_mesh::ChunkMeshData,
    mesh_generation::{
        chunk_mesh_generator_input::ChunkMeshGeneratorInput, greedy_mesher::GreedyMesher,
    },
};
pub enum ChunkMeshGeneratorCommand {
    GenerateMesh { pos: ChunkPos, chunk: Box<Chunk> },
}

pub enum ChunkMeshGeneratorEvent {
    Generated {
        pos: ChunkPos,
        mesh: Box<ChunkMeshData>,
    },
}

pub struct ChunkMeshGenerator {
    block_database: Arc<BlockDatabase>,
    //chunk_receiver: Receiver<ChunkLoaderEvent>,
    mesh_sender: Sender<ChunkMeshGeneratorEvent>,
}

impl ChunkMeshGenerator {
    pub fn new(
        block_database: Arc<BlockDatabase>,
        //chunk_receiver: Receiver<ChunkLoaderEvent>,
    ) -> (Self, Receiver<ChunkMeshGeneratorEvent>) {
        let (mesh_sender, mesh_receiver) = crossbeam_channel::unbounded();
        (
            ChunkMeshGenerator {
                block_database,
                mesh_sender,
            },
            mesh_receiver,
        )
    }

    pub fn generate_chunk_mesh(&self, world: &World, pos: ChunkPos) -> ChunkMeshData {
        let input = ChunkMeshGeneratorInput::try_from_world(world, pos)
            .expect("Tried to mesh a chunk that does not exist in the world");

        let Some(input) = input else {
            // Chunk is empty, return empty mesh
            return ChunkMeshData::from_position(pos);
        };

        let mut mesher = GreedyMesher::new(self.block_database.clone());
        mesher.generate_mesh(&input)
    }

    pub fn generate_chunk_mesh_async(&self, world: &World, pos: ChunkPos) {
        let input = ChunkMeshGeneratorInput::try_from_world(world, pos)
            .expect("Tried to mesh a chunk that does not exist in the world");

        let Some(input) = input else {
            // Empty chunk, send empty mesh
            let mesh = ChunkMeshData::from_position(pos);
            self.mesh_sender
                .send(ChunkMeshGeneratorEvent::Generated {
                    pos,
                    mesh: Box::new(mesh),
                })
                .unwrap();
            return;
        };

        // TODO: Batch requests and reuse same mesher instance
        let mut mesher = GreedyMesher::new(self.block_database.clone());
        let mesh_sender = self.mesh_sender.clone();

        rayon::spawn(move || {
            let mesh = mesher.generate_mesh(&input);
            mesh_sender
                .send(ChunkMeshGeneratorEvent::Generated {
                    pos,
                    mesh: Box::new(mesh),
                })
                .unwrap();
        });
    }
}
