use std::sync::{
    LazyLock,
    atomic::{AtomicU32, Ordering},
};

use crate::voxels::chunk::ChunkState;

pub static CHUNKS_BY_STATE: LazyLock<ChunksByState> = LazyLock::new(ChunksByState::default);

#[derive(Debug, Default)]
pub struct ChunksByState([AtomicU32; ChunkState::TOTAL_STATES]);

impl ChunksByState {
    pub fn increment(&self, state: ChunkState) {
        self.0[state as usize].fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_by(&self, state: ChunkState, amount: u32) {
        self.0[state as usize].fetch_add(amount, Ordering::Relaxed);
    }

    pub fn decrement(&self, state: ChunkState) {
        self.0[state as usize].fetch_sub(1, Ordering::Relaxed);
    }

    pub fn decrement_by(&self, state: ChunkState, amount: u32) {
        self.0[state as usize].fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn get(&self, state: ChunkState) -> u32 {
        self.0[state as usize].load(Ordering::Relaxed)
    }

    pub fn transition(&self, from: ChunkState, to: ChunkState) {
        self.decrement(from);
        self.increment(to);
    }
}

#[derive(Default)]
pub struct WorldStatistics {
    pub total_chunks: usize,
    pub approximate_memory_usage_bytes: usize,
}

impl WorldStatistics {
    pub fn new() -> Self {
        Self::default()
    }
}
