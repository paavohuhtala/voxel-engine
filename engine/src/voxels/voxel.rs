use bitfield_struct::bitfield;

use crate::assets::blocks::BlockTypeId;

#[bitfield(u16, hash = true)]
#[derive(PartialEq, Eq)]
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
    pub const GRASS: Voxel = Voxel::from_type(1);
    pub const DIRT: Voxel = Voxel::from_type(2);
    pub const GOLD: Voxel = Voxel::from_type(3);

    pub const fn is_transparent(&self) -> bool {
        // TODO: Support other transparent block types
        self.block_type() == 0
    }

    pub fn block_type_id(&self) -> BlockTypeId {
        BlockTypeId(self.block_type())
    }
}
