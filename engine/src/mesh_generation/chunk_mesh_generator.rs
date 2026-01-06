use std::sync::Arc;

use crate::{
    assets::blocks::BlockDatabaseSlim,
    voxels::{chunk::ChunkData, coord::ChunkPos},
};
use crossbeam_channel::{Receiver, Sender};

pub enum ChunkMeshGeneratorCommand {
    GenerateMesh {
        pos: ChunkPos,
        chunk: Box<ChunkData>,
    },
}

pub enum ChunkMeshGeneratorEvent {
    Generated {
        pos: ChunkPos,
        mesh: Box<ChunkMeshData>,
    },
}

pub struct ChunkMeshGenerator {
    block_database: Arc<BlockDatabaseSlim>,
    mesh_sender: Sender<ChunkMeshGeneratorEvent>,
}

impl ChunkMeshGenerator {
    pub fn new(
        block_database: Arc<BlockDatabaseSlim>,
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

    pub fn generate_chunk_mesh(&self, world: &RenderWorld, pos: ChunkPos) -> ChunkMeshData {
        let input = ChunkMeshGeneratorInput::try_from_world(world, pos)
            .expect("Tried to mesh a chunk that does not exist in the world");

        let Some(input) = input else {
            // Chunk is empty, return empty mesh
            return ChunkMeshData::from_position(pos);
        };

        let mut mesher = GreedyMesher::new(self.block_database.clone());
        mesher.generate_mesh(&input)
    }

    pub fn generate_chunk_mesh_async(&self, world: &RenderWorld, pos: ChunkPos) {
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
