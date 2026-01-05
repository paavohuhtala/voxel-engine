use std::collections::HashSet;

use glam::U8Vec3;

use crate::voxels::{coord::LocalPos, unpacked_chunk::UnpackedChunk, voxel::Voxel};

#[derive(Default, Clone)]
pub struct Palette {
    pub voxel_types: Vec<Voxel>,
}

impl Palette {
    pub fn new() -> Self {
        Palette {
            voxel_types: Vec::new(),
        }
    }

    pub fn from_voxel_types(voxels: &[Voxel]) -> Self {
        Palette {
            voxel_types: voxels.to_vec(),
        }
    }

    pub fn from_voxel_type_iterator<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Voxel>,
    {
        Palette {
            voxel_types: iter.into_iter().collect(),
        }
    }

    pub fn ensure_voxel_type(&mut self, voxel: Voxel) -> usize {
        // If this is the first voxel type, ensure we've also defined AIR
        if voxel != Voxel::AIR && self.voxel_types.is_empty() {
            self.add_voxel_type(Voxel::AIR);
        }

        if let Some(index) = self.voxel_types.iter().position(|&v| v == voxel) {
            return index;
        }

        self.add_voxel_type(voxel);
        self.voxel_types.len() - 1
    }

    fn add_voxel_type(&mut self, voxel: Voxel) {
        assert!(
            self.voxel_types.len() < 2usize.pow(16),
            "Palette cannot have more than 65536 voxel types"
        );

        self.voxel_types.push(voxel);
    }

    pub fn get_voxel_type(&self, index: usize) -> Option<Voxel> {
        self.voxel_types.get(index).copied()
    }

    pub fn get_voxel_index(&self, voxel: Voxel) -> Option<usize> {
        self.voxel_types.iter().position(|&v| v == voxel)
    }

    // How many bits are needed to index into the voxel_types
    // This can be 0, which means all voxels are the same type
    // Normaly those chunks are represented as Solid chunks,
    // but when converting between representations this can be useful
    pub fn get_packed_index_bits(&self) -> usize {
        let count = self.voxel_types.len();

        (count as f32).log2().ceil() as usize
    }
}

pub const CHUNK_SIZE: u8 = 16;
// For fast division
pub const CHUNK_SIZE_LOG2: i32 = 4;

pub const CHUNK_VOLUME: usize = (CHUNK_SIZE as usize).pow(3);

pub enum Chunk {
    Solid(Voxel),
    Packed(PackedChunk),
}

pub struct PackedChunk {
    pub palette: Palette,
    // Data is stored in YZX order
    // Each voxel is represented by an index into the palette
    // Voxels are packed tightly into 64-bit integers, with possibly unused bits at the end of each u64
    pub data: Box<[u64]>,
    pub bits_per_voxel: u8,
    pub bit_mask: u64,
}

#[derive(Debug)]
struct VoxelOffsets {
    data_index: usize,
    bit_offset: usize,
}

impl Default for PackedChunk {
    fn default() -> Self {
        Self::new()
    }
}

impl PackedChunk {
    pub fn new() -> Self {
        PackedChunk {
            palette: Palette::new(),
            data: Box::new([]),
            bits_per_voxel: 0,
            bit_mask: 0,
        }
    }

    pub fn new_with_palette(palette: Palette) -> Self {
        let mut chunk = PackedChunk {
            palette,
            data: Box::new([]),
            bits_per_voxel: 0,
            bit_mask: 0,
        };
        chunk.reallocate_if_necessary();
        chunk
    }

    pub fn reallocate_if_necessary(&mut self) {
        let needed_bits = self.palette.get_packed_index_bits();
        if needed_bits == self.bits_per_voxel as usize {
            return;
        }

        // Repack data
        let old_bits = self.bits_per_voxel as usize;
        let old_data = std::mem::replace(&mut self.data, Box::new([]));

        self.bits_per_voxel = needed_bits as u8;
        self.bit_mask = (1u64 << needed_bits) - 1;

        if needed_bits == 0 {
            return;
        }

        let voxels_per_u64 = 64 / needed_bits;
        let u64_count = CHUNK_VOLUME.div_ceil(voxels_per_u64);
        self.data = vec![0u64; u64_count].into_boxed_slice();

        // If we had data, we need to copy it over
        if old_bits > 0 {
            let old_voxels_per_u64 = 64 / old_bits;
            let old_mask = (1u64 << old_bits) - 1;

            for i in 0..CHUNK_VOLUME {
                let old_u64_index = i / old_voxels_per_u64;
                let old_sub_index = i % old_voxels_per_u64;
                let old_bit_offset = old_sub_index * old_bits;
                let val = (old_data[old_u64_index] >> old_bit_offset) & old_mask;

                let new_u64_index = i / voxels_per_u64;
                let new_sub_index = i % voxels_per_u64;
                let new_bit_offset = new_sub_index * needed_bits;

                self.data[new_u64_index] |= val << new_bit_offset;
            }
        }
    }

    fn get_storage_indices(&self, index: usize) -> VoxelOffsets {
        let bits = self.bits_per_voxel as usize;
        let voxels_per_u64 = 64 / bits;
        let data_index = index / voxels_per_u64;
        let sub_index = index % voxels_per_u64;
        let bit_offset = sub_index * bits;
        VoxelOffsets {
            data_index,
            bit_offset,
        }
    }

    fn get_packed_index(&self, index: usize) -> u16 {
        if self.bits_per_voxel == 0 {
            return 0;
        }

        let offsets = self.get_storage_indices(index);
        ((self.data[offsets.data_index] >> offsets.bit_offset) & self.bit_mask) as u16
    }

    fn set_packed_index(&mut self, index: usize, value: u16) {
        if self.bits_per_voxel == 0 {
            panic!("Cannot set packed index when bits_per_voxel is 0");
        }

        let offsets = self.get_storage_indices(index);
        let mask = self.bit_mask << offsets.bit_offset;
        let value_shifted = (value as u64 & self.bit_mask) << offsets.bit_offset;
        self.data[offsets.data_index] = (self.data[offsets.data_index] & !mask) | value_shifted;
    }

    pub fn get_voxel(&self, coord: LocalPos) -> Option<Voxel> {
        let index = Self::coord_to_index(coord);
        let palette_index = self.get_packed_index(index);
        self.palette.get_voxel_type(palette_index as usize)
    }

    pub fn set_voxel(&mut self, coord: LocalPos, voxel: Voxel) {
        let palette_index = self.palette.ensure_voxel_type(voxel);
        self.reallocate_if_necessary();
        self.set_voxel_indexed(coord, palette_index as u16);
    }

    fn set_voxel_indexed(&mut self, coord: LocalPos, palette_index: u16) {
        let index = Self::coord_to_index(coord);
        self.set_packed_index(index, palette_index);
    }

    pub fn iter_voxels(&self) -> impl Iterator<Item = (LocalPos, Voxel)> + '_ {
        let bits = self.bits_per_voxel as usize;
        let voxels_per_u64 = if bits == 0 { usize::MAX } else { 64 / bits };
        let mask = self.bit_mask;

        let mut u64_index = 0;
        let mut sub_index = 0;

        let mut x = 0;
        let mut y = 0;
        let mut z = 0;

        (0..CHUNK_VOLUME).map(move |_| {
            let coord = LocalPos::new(x, y, z);

            // Advance coordinates
            x += 1;
            if x == CHUNK_SIZE {
                x = 0;
                z += 1;
                if z == CHUNK_SIZE {
                    z = 0;
                    y += 1;
                }
            }

            let palette_index = if bits == 0 {
                0
            } else {
                let bit_offset = sub_index * bits;
                // Safety: u64_index should be within bounds if logic is correct
                let val = ((self.data[u64_index] >> bit_offset) & mask) as u16;

                sub_index += 1;
                if sub_index >= voxels_per_u64 {
                    sub_index = 0;
                    u64_index += 1;
                }
                val
            };

            let voxel = self
                .palette
                .get_voxel_type(palette_index as usize)
                .unwrap_or_default();
            (coord, voxel)
        })
    }

    /**
     * Unpack the chunk into a flat vector of voxels in YZX order.
     * More efficient than iteration, if you just need the raw data.
     */
    pub fn unpack(&self, voxels: &mut [Voxel]) {
        let bits = self.bits_per_voxel as usize;
        if bits == 0 {
            // Fast path: all voxels are the same
            let voxel = self.palette.get_voxel_type(0).unwrap_or_default();
            voxels.fill(voxel);
            return;
        }

        let voxels_per_u64 = 64 / bits;
        let mask = self.bit_mask;

        let mut index = 0;

        for &word in self.data.iter() {
            let mut current_word = word;
            for _ in 0..voxels_per_u64 {
                if index >= CHUNK_VOLUME {
                    break;
                }

                let voxel = self
                    .palette
                    .get_voxel_type((current_word & mask) as usize)
                    .unwrap_or_default();
                voxels[index] = voxel;
                current_word >>= bits;
                index += 1;
            }
        }
    }

    pub fn compact(&mut self) {
        todo!("Implement compacting the palette and remapping voxel indices");
    }

    /// Convert ChunkSpaceCoord to linear index in YZX order
    /// Note that this isn't a direct offset into the data array, as voxels are packed
    /// tightly into u64s.
    pub fn coord_to_index(coord: LocalPos) -> usize {
        let LocalPos(U8Vec3 { x, y, z }) = coord;
        if x >= CHUNK_SIZE || y >= CHUNK_SIZE || z >= CHUNK_SIZE {
            panic!("Voxel coordinates out of bounds: ({}, {}, {})", x, y, z);
        }

        (y as usize * CHUNK_SIZE as usize * CHUNK_SIZE as usize)
            + (z as usize * CHUNK_SIZE as usize)
            + x as usize
    }

    /// Convert linear index in YZX order to ChunkSpaceCoord
    pub fn index_to_coord(index: usize) -> LocalPos {
        if index >= CHUNK_VOLUME {
            panic!("Voxel index out of bounds: {}", index);
        }

        let x = (index % CHUNK_SIZE as usize) as u8;
        let z = ((index / CHUNK_SIZE as usize) % CHUNK_SIZE as usize) as u8;
        let y = (index / (CHUNK_SIZE as usize * CHUNK_SIZE as usize)) as u8;

        LocalPos(U8Vec3 { x, y, z })
    }

    pub fn pack(voxels: &[Voxel]) -> Self {
        assert!(
            voxels.len() == CHUNK_VOLUME,
            "Voxel slice must have exactly {} elements",
            CHUNK_VOLUME
        );

        // This is potentially expensive, but ensures we have unique voxel types
        let voxel_set = voxels.iter().cloned().collect::<HashSet<_>>();
        let palette = Palette::from_voxel_type_iterator(voxel_set);
        let mut chunk = Self::new_with_palette(palette);

        for (i, voxel) in voxels.iter().enumerate() {
            let palette_index = chunk
                .palette
                .voxel_types
                .iter()
                .position(|&v| v == *voxel)
                .unwrap() as u16;

            chunk.set_packed_index(i, palette_index);
        }

        chunk
    }

    // Completely fill the chunk with a single voxel type
    // This is used when converting from solid to packed representation
    pub fn fill(&mut self, voxel: Voxel) {
        let palette = Palette::from_voxel_types(&[voxel]);
        *self = PackedChunk::new_with_palette(palette);
    }
}

impl Chunk {
    pub fn solid(voxel: Voxel) -> Self {
        Chunk::Solid(voxel)
    }

    pub fn empty() -> Self {
        Chunk::Packed(PackedChunk {
            palette: Palette::new(),
            data: Box::new([]),
            bits_per_voxel: 0,
            bit_mask: 0,
        })
    }

    pub fn new_with_palette(palette: Palette) -> Self {
        Chunk::Packed(PackedChunk::new_with_palette(palette))
    }

    pub fn reallocate_if_necessary(&mut self) {
        if let Chunk::Packed(packed) = self {
            packed.reallocate_if_necessary();
        }
    }

    pub fn get_voxel(&self, coord: LocalPos) -> Option<Voxel> {
        match self {
            Chunk::Solid(voxel) => Some(*voxel),
            Chunk::Packed(packed) => packed.get_voxel(coord),
        }
    }

    pub fn set_voxel(&mut self, coord: LocalPos, voxel: Voxel) {
        match self {
            Chunk::Solid(current_voxel) if *current_voxel == voxel => {
                // No change needed
            }
            Chunk::Solid(_) => {
                // Change to packed representation
                let packed = self.change_to_packed();
                packed.set_voxel(coord, voxel);
            }
            Chunk::Packed(packed) => {
                packed.set_voxel(coord, voxel);
            }
        }
    }

    fn change_to_packed(&mut self) -> &mut PackedChunk {
        if let Chunk::Solid(voxel) = *self {
            let mut palette = Palette::new();
            palette.add_voxel_type(voxel);

            *self = Chunk::Packed(PackedChunk {
                palette,
                data: Box::new([]),
                bits_per_voxel: 0,
                bit_mask: 0,
            });

            self.reallocate_if_necessary();

            if let Chunk::Packed(packed) = self {
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
            Chunk::Solid(_) => None,
            Chunk::Packed(packed) => Some(packed),
        }
    }

    pub fn as_packed_mut(&mut self) -> Option<&mut PackedChunk> {
        match self {
            Chunk::Solid(_) => None,
            Chunk::Packed(packed) => Some(packed),
        }
    }

    pub fn bits_per_voxel(&self) -> u8 {
        match self {
            Chunk::Solid(_) => 0,
            Chunk::Packed(packed) => {
                // TODO: This isn't guaranteed to be up-to-date if palette has changed without allocation
                packed.bits_per_voxel
            }
        }
    }

    pub fn iter_voxels(&self) -> Box<dyn Iterator<Item = (LocalPos, Voxel)> + '_> {
        match self {
            Chunk::Solid(voxel) => {
                let iter = (0..CHUNK_VOLUME).map(move |i| {
                    let coord = PackedChunk::index_to_coord(i);
                    (coord, *voxel)
                });
                Box::new(iter)
            }
            Chunk::Packed(packed) => Box::new(packed.iter_voxels()),
        }
    }
}

impl From<PackedChunk> for Chunk {
    fn from(value: PackedChunk) -> Self {
        Chunk::Packed(value)
    }
}

impl From<&[Voxel]> for Chunk {
    fn from(data: &[Voxel]) -> Self {
        // If all voxels are the same, return a solid chunk
        let first = data[0];
        let all_same = data.iter().all(|&v| v == first);
        if all_same {
            Chunk::Solid(first)
        } else {
            Chunk::Packed(PackedChunk::pack(data))
        }
    }
}

impl From<UnpackedChunk> for Chunk {
    fn from(value: UnpackedChunk) -> Self {
        Chunk::from(value.voxels.as_slice())
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

        let mut chunk = Chunk::solid(air);
        // Initially 0 bits per voxel (only air)
        assert_eq!(chunk.bits_per_voxel(), 0);
        // Representation is solid
        assert!(matches!(chunk, Chunk::Solid(_)));

        // Two types -> 1 bit required
        chunk.set_voxel(LocalPos::new(0, 0, 0), stone);
        assert_eq!(chunk.bits_per_voxel(), 1);
        // And representation changed to packed
        assert!(matches!(chunk, Chunk::Packed(_)));

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
        let mut chunk = Chunk::solid(Voxel::AIR);
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
        let mut chunk = Chunk::solid(Voxel::AIR);

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
            Chunk::Packed(p) => p.iter_voxels().collect(),
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
        let mut chunk = Chunk::solid(Voxel::AIR);
        let stone = Voxel::from_type(1);

        // Convert to packed and fill
        let packed = chunk.change_to_packed();
        packed.fill(stone);

        // Verify all voxels are stone
        if let Chunk::Packed(packed) = &chunk {
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
