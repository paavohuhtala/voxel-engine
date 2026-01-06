use std::{
    mem::size_of,
    sync::atomic::{AtomicU8, Ordering},
};

use crate::{
    mesh_generation::chunk_mesh::ChunkMeshData,
    voxels::{
        coord::{ChunkPos, LocalPos},
        face::Face,
        packed_chunk::{PackedChunk, Palette},
        unpacked_chunk::UnpackedChunk,
        voxel::Voxel,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkState {
    WaitingForLoading,
    Generating,
    // TODO: Add LoadingFromDisk state here
    Loaded,
    // TODO: Add decoration step here
    Meshing,
    Ready,
}

impl ChunkState {
    pub fn all() -> &'static [ChunkState] {
        &[
            ChunkState::WaitingForLoading,
            ChunkState::Generating,
            ChunkState::Loaded,
            ChunkState::Meshing,
            ChunkState::Ready,
        ]
    }
}

/// Contains status flags about chunk neighbors for meshing purposes.
#[derive(Default)]
pub struct ChunkNeighborStatus(AtomicU8);

impl ChunkNeighborStatus {
    /// Marks the neighbor in the given direction as ready.
    /// Returns true if all neighbors are now ready.
    pub fn set_neighbor_ready(&self, direction: Face) -> bool {
        let mask = 1 << (direction as u8);
        let previous = self.0.fetch_or(mask, Ordering::SeqCst);
        (previous | mask) == 0b0011_1111
    }

    pub fn set_neighbor_bits(&self, bits: u8) -> bool {
        let previous = self.0.fetch_or(bits, Ordering::SeqCst);
        (previous | bits) == 0b0011_1111
    }
}

pub struct Chunk<T: IChunkRenderState = ()> {
    pub position: ChunkPos,
    pub data: Option<ChunkData>,
    // TODO: Wrap this in crossbeam::atomic::AtomicCell?
    pub state: ChunkState,
    pub render_state: Option<T>,
    pub neighbor_status: ChunkNeighborStatus,
}

pub trait IChunkRenderContext: Send + Clone {
    fn flush(&mut self, on_done: Box<dyn FnOnce() + Send>);
}

pub trait IChunkRenderState: Send + Sync + 'static {
    type Context: IChunkRenderContext;
    fn create_and_upload_mesh(context: &mut Self::Context, mesh_data: ChunkMeshData) -> Self;
    fn chunk_gpu_id(&self) -> u64;
}

impl IChunkRenderContext for () {
    fn flush(&mut self, on_done: Box<dyn FnOnce() + Send>) {
        on_done();
    }
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
        Chunk {
            position,
            data: None,
            state: ChunkState::WaitingForLoading,
            render_state: None,
            neighbor_status: ChunkNeighborStatus::default(),
        }
    }

    pub fn from_data(position: ChunkPos, data: ChunkData) -> Self {
        Chunk {
            position,
            data: Some(data),
            state: ChunkState::Loaded,
            render_state: None,
            neighbor_status: ChunkNeighborStatus::default(),
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
        matches!(
            self.state,
            ChunkState::Loaded | ChunkState::Meshing | ChunkState::Ready
        )
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
}
