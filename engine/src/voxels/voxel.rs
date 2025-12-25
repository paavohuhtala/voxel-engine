use bitfield_struct::bitfield;

#[bitfield(u16, hash = true)]
pub struct Voxel {
    #[bits(12)]
    pub block_type: u16,
    #[bits(4)]
    pub metadata: u8,
}

impl Voxel {
    pub const fn from_type(block_type: u16) -> Self {
        let mut voxel = Voxel::new();
        voxel.set_block_type(block_type);
        voxel
    }

    pub const fn from_type_metadata(block_type: u16, metadata: u8) -> Self {
        let mut voxel = Voxel::new();
        voxel.set_block_type(block_type);
        voxel.set_metadata(metadata);
        voxel
    }

    pub const AIR: Voxel = Voxel::new();
    pub const STONE: Voxel = Voxel::from_type(1);

    pub const fn is_transparent(&self) -> bool {
        // TODO: Support other transparent block types
        self.block_type() == 0
    }
}

impl PartialEq for Voxel {
    fn eq(&self, other: &Self) -> bool {
        self.into_bits() == other.into_bits()
    }
}

impl Eq for Voxel {}
