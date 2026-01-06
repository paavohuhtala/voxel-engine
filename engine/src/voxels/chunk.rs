use std::{
    fmt::Debug,
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use crossbeam::atomic::AtomicCell;

use crate::{
    mesh_generation::chunk_mesh::ChunkMeshData,
    voxels::{
        coord::{ChunkPos, LocalPos},
        face::Face,
        packed_chunk::{PackedChunk, Palette},
        unpacked_chunk::UnpackedChunk,
        voxel::Voxel,
    },
    world_stats::CHUNKS_BY_STATE,
};

pub const CHUNK_SIZE: u8 = 16;
// For fast division
pub const CHUNK_SIZE_LOG2: i32 = 4;

pub const CHUNK_VOLUME: usize = (CHUNK_SIZE as usize).pow(3);

pub enum ChunkData {
    Solid(Voxel),
    Packed(PackedChunk),
}

impl ChunkData {
    pub fn solid(voxel: Voxel) -> Self {
        ChunkData::Solid(voxel)
    }

    pub fn empty() -> Self {
        ChunkData::Packed(PackedChunk {
            palette: Palette::new(),
            data: Box::new([]),
            bits_per_voxel: 0,
            bit_mask: 0,
        })
    }

    pub fn new_with_palette(palette: Palette) -> Self {
        ChunkData::Packed(PackedChunk::new_with_palette(palette))
    }

    pub fn reallocate_if_necessary(&mut self) {
        if let ChunkData::Packed(packed) = self {
            packed.reallocate_if_necessary();
        }
    }

    pub fn get_voxel(&self, coord: LocalPos) -> Option<Voxel> {
        match self {
            ChunkData::Solid(voxel) => Some(*voxel),
            ChunkData::Packed(packed) => packed.get_voxel(coord),
        }
    }

    pub fn set_voxel(&mut self, coord: LocalPos, voxel: Voxel) {
        match self {
            ChunkData::Solid(current_voxel) if *current_voxel == voxel => {
                // No change needed
            }
            ChunkData::Solid(_) => {
                // Change to packed representation
                let packed = self.change_to_packed();
                packed.set_voxel(coord, voxel);
            }
            ChunkData::Packed(packed) => {
                packed.set_voxel(coord, voxel);
            }
        }
    }

    fn change_to_packed(&mut self) -> &mut PackedChunk {
        if let ChunkData::Solid(voxel) = *self {
            let mut palette = Palette::new();
            palette.add_voxel_type(voxel);

            *self = ChunkData::Packed(PackedChunk {
                palette,
                data: Box::new([]),
                bits_per_voxel: 0,
                bit_mask: 0,
            });

            self.reallocate_if_necessary();

            if let ChunkData::Packed(packed) = self {
                packed
            } else {
                unreachable!();
            }
        } else {
            panic!("Chunk is already packed");
        }
    }

    pub fn as_packed(&self) -> Option<&PackedChunk> {
        match self {
            ChunkData::Solid(_) => None,
            ChunkData::Packed(packed) => Some(packed),
        }
    }

    pub fn as_packed_mut(&mut self) -> Option<&mut PackedChunk> {
        match self {
            ChunkData::Solid(_) => None,
            ChunkData::Packed(packed) => Some(packed),
        }
    }

    pub fn bits_per_voxel(&self) -> u8 {
        match self {
            ChunkData::Solid(_) => 0,
            ChunkData::Packed(packed) => {
                // TODO: This isn't guaranteed to be up-to-date if palette has changed without allocation
                packed.bits_per_voxel
            }
        }
    }

    pub fn iter_voxels(&self) -> Box<dyn Iterator<Item = (LocalPos, Voxel)> + '_> {
        match self {
            ChunkData::Solid(voxel) => {
                let iter = (0..CHUNK_VOLUME).map(move |i| {
                    let coord = PackedChunk::index_to_coord(i);
                    (coord, *voxel)
                });
                Box::new(iter)
            }
            ChunkData::Packed(packed) => Box::new(packed.iter_voxels()),
        }
    }

    pub fn approximate_size(&self) -> usize {
        match self {
            ChunkData::Solid(_) => size_of::<Self>(),
            ChunkData::Packed(packed) => size_of::<Self>() + packed.approximate_size(),
        }
    }
}

impl From<PackedChunk> for ChunkData {
    fn from(value: PackedChunk) -> Self {
        ChunkData::Packed(value)
    }
}

impl From<&[Voxel]> for ChunkData {
    fn from(data: &[Voxel]) -> Self {
        // If all voxels are the same, return a solid chunk
        let first = data[0];
        let all_same = data.iter().all(|&v| v == first);
        if all_same {
            ChunkData::Solid(first)
        } else {
            ChunkData::Packed(PackedChunk::pack(data))
        }
    }
}

impl From<UnpackedChunk> for ChunkData {
    fn from(value: UnpackedChunk) -> Self {
        ChunkData::from(value.voxels.as_slice())
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChunkState {
    /// Chunk has been initialized but only contains flags
    Initial = 0,
    /// Chunk is queued for world generation
    InGenerationQueue,
    /// Chunk is being generated
    Generating,
    // TODO: Add LoadingFromDisk state here
    // TODO: Add StaleMesh state here
    /// Chunk has been generated and voxel data is available
    Loaded,
    // TODO: Add decoration step here
    /// Chunk is queued for meshing
    InMeshingQueue,
    /// Chunk is being meshed
    Meshing,
    /// Chunk mesh is finished, but is currently waiting for flush to renderer
    WaitingForRendererFlush,
    /// Chunk is ready to render
    Ready,
    /// Chunk is ready to render, but has no voxel data.
    /// The chunk contains only air, or it's fully occluded by neighboring chunks.
    ReadyEmpty,
    // TODO: Add separate ReadyOccluded state here
    /// Chunk has been unloaded. This is a terminal state - any further state transitions are ignored.
    /// This state is not tracked in statistics.
    /// While unloaded chunks are removed from the map, handles continue to exist and can point to this state.
    Unloaded,
}

impl ChunkState {
    pub const TOTAL_STATES: usize = 10;

    pub const fn all() -> &'static [ChunkState] {
        &[
            ChunkState::Initial,
            ChunkState::InGenerationQueue,
            ChunkState::Generating,
            ChunkState::Loaded,
            ChunkState::InMeshingQueue,
            ChunkState::Meshing,
            ChunkState::WaitingForRendererFlush,
            ChunkState::Ready,
            ChunkState::ReadyEmpty,
            ChunkState::Unloaded,
        ]
    }
}

/// Contains a bit mask representing which neighboring chunks have been generated.
#[derive(Default)]
pub struct ChunkNeighborState(AtomicU8);

impl ChunkNeighborState {
    const ALL_NEIGHBORS_MASK: u8 = 0b0011_1111;

    /// Marks the neighbor in the given direction as ready.
    /// Returns true if all neighbors are now ready (and this call set the final bit).
    pub fn set_neighbor_ready(&self, direction: Face) -> bool {
        let mask = 1 << (direction as u8);
        let previous = self.0.fetch_or(mask, Ordering::SeqCst);
        (previous | mask) == Self::ALL_NEIGHBORS_MASK
    }

    /// Sets multiple neighbor bits at once.
    /// Returns true if all neighbors are now ready (and this call completed the mask).
    pub fn set_neighbor_bits(&self, bits: u8) -> bool {
        let previous = self.0.fetch_or(bits, Ordering::SeqCst);
        (previous | bits) == Self::ALL_NEIGHBORS_MASK
    }

    /// Clears the neighbor-ready bit for the given direction.
    pub fn clear_neighbor_ready(&self, direction: Face) {
        let mask = !(1 << (direction as u8));
        self.0.fetch_and(mask, Ordering::SeqCst);
    }

    /// Clears multiple neighbor bits at once.
    pub fn clear_neighbor_bits(&self, bits: u8) {
        self.0.fetch_and(!bits, Ordering::SeqCst);
    }

    pub fn is_ready_for_meshing(&self) -> bool {
        self.0.load(Ordering::SeqCst) == Self::ALL_NEIGHBORS_MASK
    }

    pub fn load(&self) -> u8 {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct Chunk<T: IChunkRenderState = ()> {
    pub position: ChunkPos,
    pub data: Option<ChunkData>,
    pub state: Arc<AtomicCell<ChunkState>>,
    pub render_state: Option<T>,
    pub neighbor_state: Arc<ChunkNeighborState>,
}

#[derive(Clone)]
pub struct ChunkHandle {
    pub pos: ChunkPos,
    state: Arc<AtomicCell<ChunkState>>,
    pub neighbor_state: Arc<ChunkNeighborState>,
}

impl Debug for ChunkHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkHandle")
            .field("pos", &self.pos)
            .field("state", &self.state.load())
            .finish()
    }
}

impl ChunkHandle {
    pub fn state(&self) -> ChunkState {
        self.state.load()
    }

    /// Attempts a single atomic state transition from `from` to `to`.
    /// Returns true only if the transition succeeded.
    pub fn try_transition(&self, from: ChunkState, to: ChunkState) -> bool {
        if from == to {
            return false;
        }

        // Don't transition unloaded chunks
        if self.state.load() == ChunkState::Unloaded {
            return false;
        }

        if self.state.compare_exchange(from, to).is_ok() {
            CHUNKS_BY_STATE.transition(from, to);
            true
        } else {
            false
        }
    }

    /// Sets the state of the chunk and updates the global statistics.
    /// If the chunk has already been unloaded, this is a no-op.
    pub fn set_state(&self, new_state: ChunkState) {
        loop {
            let old_state = self.state.load();
            if old_state == ChunkState::Unloaded {
                // Chunk was unloaded, ignore any further state transitions
                return;
            }
            if old_state == new_state {
                // Avoid no-op transitions (would corrupt statistics)
                return;
            }
            if self.state.compare_exchange(old_state, new_state).is_ok() {
                CHUNKS_BY_STATE.transition(old_state, new_state);
                return;
            }
            // Another thread changed the state, retry
        }
    }
}

pub trait IChunkRenderContext: Send + Clone {
    type FlushResult: Debug + Send + 'static;
    fn flush(&mut self) -> Self::FlushResult;
}

pub trait IChunkRenderState: Send + Sync + 'static {
    type Context: IChunkRenderContext;
    fn create_and_upload_mesh(context: &mut Self::Context, mesh_data: ChunkMeshData) -> Self;
    fn chunk_gpu_id(&self) -> u64;
}

impl IChunkRenderContext for () {
    type FlushResult = ();

    fn flush(&mut self) -> Self::FlushResult {}
}

impl IChunkRenderState for () {
    type Context = ();
    fn create_and_upload_mesh<'a>(_context: &mut Self::Context, _mesh_data: ChunkMeshData) -> Self {
    }
    fn chunk_gpu_id(&self) -> u64 {
        0
    }
}

impl<T: IChunkRenderState> Chunk<T> {
    pub fn new(position: ChunkPos) -> Self {
        CHUNKS_BY_STATE.increment(ChunkState::Initial);

        Chunk {
            position,
            data: None,
            state: Arc::new(AtomicCell::new(ChunkState::Initial)),
            render_state: None,
            neighbor_state: Arc::default(),
        }
    }

    pub fn from_data(position: ChunkPos, data: ChunkData) -> Self {
        CHUNKS_BY_STATE.increment(ChunkState::Loaded);

        Chunk {
            position,
            data: Some(data),
            state: Arc::new(AtomicCell::new(ChunkState::Loaded)),
            render_state: None,
            neighbor_state: Arc::default(),
        }
    }

    pub fn set_voxel(&mut self, pos: LocalPos, voxel: Voxel) {
        if let Some(chunk_data) = &mut self.data {
            chunk_data.set_voxel(pos, voxel);
        } else {
            panic!("Tried to set voxel on chunk with no data");
        }
    }

    pub fn get_voxel(&self, pos: LocalPos) -> Option<Voxel> {
        if let Some(chunk_data) = &self.data {
            chunk_data.get_voxel(pos)
        } else {
            None
        }
    }

    pub fn approximate_size(&self) -> usize {
        // This only counts CPU memory, add separate method for GPU memory
        size_of::<Self>()
            + match &self.data {
                Some(data) => data.approximate_size(),
                None => 0,
            }
    }

    pub fn is_suitable_neighbor_for_meshing(&self) -> bool {
        let state = self.state.load();
        self.data.is_some() && state < ChunkState::Unloaded
    }

    pub fn handle(&self) -> ChunkHandle {
        ChunkHandle {
            pos: self.position,
            state: self.state.clone(),
            neighbor_state: self.neighbor_state.clone(),
        }
    }

    pub fn is_ready_for_meshing(&self) -> bool {
        self.data.is_some() && self.neighbor_state.is_ready_for_meshing()
    }

    /// Marks the chunk as unloaded and decrements the statistics for the current state.
    /// This uses compare-exchange to prevent race conditions with set_state on handles.
    pub fn unload(&self) {
        loop {
            let old_state = self.state.load();
            if old_state == ChunkState::Unloaded {
                // Already unloaded
                return;
            }
            if self
                .state
                .compare_exchange(old_state, ChunkState::Unloaded)
                .is_ok()
            {
                CHUNKS_BY_STATE.decrement(old_state);
                return;
            }
            // Another thread changed the state, retry
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_voxel_storage() {
        let air = Voxel::AIR;
        let stone = Voxel::from_type(1);
        let dirt = Voxel::from_type(2);
        let grass = Voxel::from_type(3);
        let watermelon = Voxel::from_type(4);

        let mut chunk = ChunkData::solid(air);
        // Initially 0 bits per voxel (only air)
        assert_eq!(chunk.bits_per_voxel(), 0);
        // Representation is solid
        assert!(matches!(chunk, ChunkData::Solid(_)));

        // Two types -> 1 bit required
        chunk.set_voxel(LocalPos::new(0, 0, 0), stone);
        assert_eq!(chunk.bits_per_voxel(), 1);
        // And representation changed to packed
        assert!(matches!(chunk, ChunkData::Packed(_)));

        // Three types -> 2 bits required
        chunk.set_voxel(LocalPos::new(1, 0, 0), dirt);
        assert_eq!(chunk.bits_per_voxel(), 2);

        // Four types -> 2 bits still sufficient
        chunk.set_voxel(LocalPos::new(2, 0, 0), grass);
        assert_eq!(chunk.bits_per_voxel(), 2);

        // Five types -> 3 bits required
        chunk.set_voxel(LocalPos::new(3, 0, 0), watermelon);
        assert_eq!(chunk.bits_per_voxel(), 3);

        // Can read back values correctly
        assert_eq!(chunk.get_voxel(LocalPos::new(0, 0, 0)), Some(stone));
        assert_eq!(chunk.get_voxel(LocalPos::new(1, 0, 0)), Some(dirt));
        assert_eq!(chunk.get_voxel(LocalPos::new(2, 0, 0)), Some(grass));
        assert_eq!(chunk.get_voxel(LocalPos::new(3, 0, 0)), Some(watermelon));
        assert_eq!(chunk.get_voxel(LocalPos::new(4, 0, 0)), Some(air));
    }

    #[test]
    fn test_chunk_boundary_crossing() {
        let mut chunk = ChunkData::solid(Voxel::AIR);
        chunk.change_to_packed();

        // Add enough types to get 5 bits per voxel (32 types)
        for i in 1..33 {
            chunk
                .as_packed_mut()
                .unwrap()
                .palette
                .add_voxel_type(Voxel::from_type(i as u16));
            chunk.reallocate_if_necessary();
        }
        assert_eq!(chunk.bits_per_voxel(), 6); // log2(33) = 5.04 -> 6 bits

        // 6 bits per voxel.
        // 64 / 6 = 10 voxels per u64.
        // Voxel 10 (index 10) should be in the second u64 (index 1).
        let coord_at_boundary = PackedChunk::index_to_coord(10);
        let voxel_val = Voxel::from_type(5);
        chunk.set_voxel(coord_at_boundary, voxel_val);

        assert_eq!(chunk.get_voxel(coord_at_boundary), Some(voxel_val));
        let packed = chunk.as_packed().unwrap();
        let offsets = packed.get_storage_indices(10);
        assert_eq!(
            offsets.data_index, 1,
            "Voxel 10 should be in the second u64"
        );
        assert_eq!(
            offsets.bit_offset, 0,
            "Voxel 10 should be at the start of the u64"
        );
        let palette_index = packed.palette.get_voxel_index(voxel_val).unwrap();
        let stored_val = packed.get_packed_index(10);
        assert_eq!(stored_val, palette_index as u16);
    }

    #[test]
    fn test_iter_voxels_order() {
        let mut chunk = ChunkData::solid(Voxel::AIR);

        // Set a few voxels at known positions
        let p1 = LocalPos::new(0, 0, 0);
        let p2 = LocalPos::new(15, 0, 0); // End of first row
        let p3 = LocalPos::new(0, 0, 1); // Start of second row (z moves)
        let p4 = LocalPos::new(0, 1, 0); // Start of second layer (y moves)
        let p5 = LocalPos::new(15, 15, 15); // Last voxel

        let v1 = Voxel::from_type(1);
        let v2 = Voxel::from_type(2);
        let v3 = Voxel::from_type(3);
        let v4 = Voxel::from_type(4);
        let v5 = Voxel::from_type(5);

        chunk.set_voxel(p1, v1);
        chunk.set_voxel(p2, v2);
        chunk.set_voxel(p3, v3);
        chunk.set_voxel(p4, v4);
        chunk.set_voxel(p5, v5);

        let collected: Vec<(LocalPos, Voxel)> = match &chunk {
            ChunkData::Packed(p) => p.iter_voxels().collect(),
            _ => panic!("Chunk should be packed"),
        };

        assert_eq!(collected.len(), CHUNK_VOLUME);

        // Check specific indices based on YZX order
        // Index = y*256 + z*16 + x

        // p1: (0,0,0) -> index 0
        assert_eq!(collected[0].0, p1);
        assert_eq!(collected[0].1, v1);

        // p2: (15,0,0) -> index 15
        assert_eq!(collected[15].0, p2);
        assert_eq!(collected[15].1, v2);

        // p3: (0,0,1) -> index 16
        assert_eq!(collected[16].0, p3);
        assert_eq!(collected[16].1, v3);

        // p4: (0,1,0) -> index 256
        assert_eq!(collected[256].0, p4);
        assert_eq!(collected[256].1, v4);

        // p5: (15,15,15) -> index 4095
        assert_eq!(collected[4095].0, p5);
        assert_eq!(collected[4095].1, v5);
    }

    #[test]
    fn test_fill_chunk() {
        let mut chunk = ChunkData::solid(Voxel::AIR);
        let stone = Voxel::from_type(1);

        // Convert to packed and fill
        let packed = chunk.change_to_packed();
        packed.fill(stone);

        // Verify all voxels are stone
        if let ChunkData::Packed(packed) = &chunk {
            for (_, voxel) in packed.iter_voxels() {
                assert_eq!(voxel, stone);
            }
            // Should only have 1 voxel type in palette (0 bits per voxel)
            assert_eq!(packed.bits_per_voxel, 0);
            assert_eq!(packed.palette.voxel_types.len(), 1);
        } else {
            panic!("Chunk should be packed");
        }
    }

    #[test]
    fn test_neighbor_bits_can_be_cleared() {
        let state = ChunkNeighborState::default();
        assert!(!state.is_ready_for_meshing());

        // Set all neighbors
        state.set_neighbor_bits(0b0011_1111);
        assert!(state.is_ready_for_meshing());

        // Clear one bit and ensure readiness drops
        state.clear_neighbor_ready(Face::Right);
        assert!(!state.is_ready_for_meshing());

        // Re-set it and ensure readiness returns
        state.set_neighbor_ready(Face::Right);
        assert!(state.is_ready_for_meshing());
    }
}
