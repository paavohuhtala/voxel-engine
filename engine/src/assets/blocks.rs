use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

use crate::{
    assets::world_textures::{TextureTransparency, WorldTextureHandle, WorldTextures},
    voxels::face::Face,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockTypeId(pub u16);

pub struct BlockDatabaseEntry {
    pub id: BlockTypeId,
    pub name: String,
    pub texture_indices: Option<TextureIndices>,
}

#[derive(Debug, Clone, Copy)]
pub struct TextureIndices {
    pub top: WorldTextureHandle,
    pub bottom: WorldTextureHandle,
    pub side: WorldTextureHandle,
}

impl TextureIndices {
    pub fn new_single(index: WorldTextureHandle) -> Self {
        TextureIndices {
            top: index,
            bottom: index,
            side: index,
        }
    }

    pub fn get_face_index(&self, face: Face) -> u16 {
        match face {
            Face::Top => self.top.0,
            Face::Bottom => self.bottom.0,
            _ => self.side.0,
        }
    }
}

pub struct TextureIndex(pub u16);

#[derive(Debug, Clone, Deserialize)]
pub enum BlockTextureDefinition {
    Invisible,
    Single(String),
    PerFace {
        top: String,
        bottom: String,
        side: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockDefinition {
    pub id: u16,
    pub name: String,
    pub textures: BlockTextureDefinition,
    pub transparency: Option<TextureTransparency>,
}

pub struct BlockDatabase {
    blocks: Vec<BlockDatabaseEntry>,
    // TODO: Should BlockDatabase really own WorldTextures?
    pub world_textures: WorldTextures,
    assets_root: PathBuf,
}

impl BlockDatabase {
    pub fn new() -> Self {
        let assets_root = PathBuf::from("assets");

        if !assets_root.exists() {
            log::error!(
                "Assets root path '{}' does not exist - check your working directory!",
                assets_root.display()
            );
        }

        BlockDatabase {
            assets_root,
            blocks: Vec::new(),
            world_textures: WorldTextures::new(),
        }
    }

    fn load_from_defs(&mut self, defs: Vec<BlockDefinition>) -> anyhow::Result<()> {
        // TODO: Load textures asynchronously
        for def in defs {
            self.add_block_from_definition(def)?;
        }
        log::info!("Loaded {} block definitions", self.blocks.len());
        Ok(())
    }

    pub fn add_block_from_definition(
        &mut self,
        block: BlockDefinition,
    ) -> anyhow::Result<BlockTypeId> {
        let transparency = block.transparency.unwrap_or(TextureTransparency::Opaque);

        let indices = match block.textures {
            BlockTextureDefinition::Invisible => None,
            BlockTextureDefinition::Single(single) => {
                let index = self
                    .world_textures
                    .load_from_path_and_allocate(&single, transparency)?;
                Some(TextureIndices::new_single(index))
            }
            BlockTextureDefinition::PerFace { top, bottom, side } => {
                let top_index = self
                    .world_textures
                    .load_from_path_and_allocate(&top, transparency)?;
                let bottom_index = self
                    .world_textures
                    .load_from_path_and_allocate(&bottom, transparency)?;
                let side_index = self
                    .world_textures
                    .load_from_path_and_allocate(&side, transparency)?;
                Some(TextureIndices {
                    top: top_index,
                    bottom: bottom_index,
                    side: side_index,
                })
            }
        };

        let block_entry = BlockDatabaseEntry {
            id: BlockTypeId(block.id),
            name: block.name,
            texture_indices: indices,
        };

        self.blocks.push(block_entry);
        Ok(BlockTypeId(block.id))
    }

    pub fn load_all_blocks(&mut self) -> anyhow::Result<()> {
        let blocks_path = self.assets_root.join("defs/blocks.ron");
        let defs_data =
            std::fs::read_to_string(&blocks_path).context("Failed to read block defs file")?;
        let defs: Vec<BlockDefinition> =
            ron::from_str(&defs_data).context("Failed to parse block defs file")?;
        self.load_from_defs(defs)?;
        Ok(())
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = &BlockDatabaseEntry> {
        self.blocks.iter()
    }

    pub fn get_by_id(&self, id: BlockTypeId) -> Option<&BlockDatabaseEntry> {
        self.blocks.iter().find(|b| b.id == id)
    }
}

impl Default for BlockDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal version of block database for use in voxel meshing where only block ID -> texture index mapping is needed
pub struct BlockDatabaseSlim {
    blocks: Vec<TextureIndices>,
}

impl BlockDatabaseSlim {
    pub fn new() -> Self {
        BlockDatabaseSlim { blocks: Vec::new() }
    }

    /// This is only for testing purposes - in normal operation, BlockDatabaseSlim is always created from a full BlockDatabase
    pub fn add_block(&mut self, indices: TextureIndices) -> BlockTypeId {
        self.blocks.push(indices);
        BlockTypeId((self.blocks.len() - 1) as u16)
    }

    pub fn from_block_database(db: &BlockDatabase) -> Self {
        let blocks = db
            .blocks
            .iter()
            .map(|b| {
                b.texture_indices
                    .unwrap_or(TextureIndices::new_single(WorldTextureHandle::ERROR))
            })
            .collect::<Vec<_>>();
        BlockDatabaseSlim { blocks }
    }

    pub fn get_texture_indices(&self, id: BlockTypeId) -> Option<&TextureIndices> {
        self.blocks.get(id.0 as usize)
    }
}

impl Default for BlockDatabaseSlim {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&BlockDatabase> for BlockDatabaseSlim {
    fn from(db: &BlockDatabase) -> Self {
        BlockDatabaseSlim::from_block_database(db)
    }
}
