use std::{collections::HashSet, mem::size_of};

use glam::U8Vec3;

use super::{
    chunk::{CHUNK_SIZE, CHUNK_VOLUME},
    coord::LocalPos,
    voxel::Voxel,
};

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

    pub(crate) fn add_voxel_type(&mut self, voxel: Voxel) {
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

    pub fn approximate_size(&self) -> usize {
        size_of::<Self>() + self.voxel_types.capacity() * size_of::<Voxel>()
    }
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
pub(crate) struct VoxelOffsets {
    pub data_index: usize,
    pub bit_offset: usize,
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

    pub(crate) fn get_storage_indices(&self, index: usize) -> VoxelOffsets {
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

    pub(crate) fn get_packed_index(&self, index: usize) -> u16 {
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

    pub fn approximate_size(&self) -> usize {
        size_of::<Self>() + self.data.len() * size_of::<u64>() + self.palette.approximate_size()
    }
}
