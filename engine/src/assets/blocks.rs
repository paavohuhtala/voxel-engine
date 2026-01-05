use std::{
    path::{Path, PathBuf},
    sync::RwLock,
};

use anyhow::Context;
use image::RgbaImage;
use serde::Deserialize;

use crate::voxels::face::Face;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockTypeId(pub u16);

pub struct BlockDatabaseEntry {
    pub id: BlockTypeId,
    pub name: String,
    textures: BlockTextures,
    texture_indices: RwLock<Option<TextureIndices>>,
}

impl BlockDatabaseEntry {
    pub fn get_texture_indices(&self) -> Option<TextureIndices> {
        self.texture_indices.read().unwrap().to_owned()
    }

    pub fn set_texture_indices(&self, indices: TextureIndices) {
        *self.texture_indices.write().unwrap() = Some(indices);
    }

    pub fn get_textures(&self) -> &BlockTextures {
        &self.textures
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextureIndices {
    pub top: u16,
    pub bottom: u16,
    pub side: u16,
}

impl TextureIndices {
    pub fn new_single(index: u16) -> Self {
        TextureIndices {
            top: index,
            bottom: index,
            side: index,
        }
    }

    pub fn get_face_index(&self, face: Face) -> u16 {
        match face {
            Face::Top => self.top,
            Face::Bottom => self.bottom,
            _ => self.side,
        }
    }
}

pub enum BlockTextures {
    Invisible,
    Single(RgbaImage),
    PerFace {
        top: RgbaImage,
        bottom: RgbaImage,
        side: RgbaImage,
    },
}

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
}

pub struct BlockDatabase {
    blocks: Vec<BlockDatabaseEntry>,
    assets_root: PathBuf,
}

impl Default for BlockDatabase {
    fn default() -> Self {
        Self::new()
    }
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
        let texture_path = self.assets_root.join("textures");

        let textures: BlockTextures = match block.textures {
            BlockTextureDefinition::Invisible => BlockTextures::Invisible,
            BlockTextureDefinition::Single(single) => {
                let texture_image = Self::load_texture(&texture_path.join(single))?;
                BlockTextures::Single(texture_image)
            }
            BlockTextureDefinition::PerFace { top, bottom, side } => {
                // TODO: If some of the images are the same, avoid loading them multiple times
                let top_image = Self::load_texture(&texture_path.join(top)).with_context(|| {
                    format!("Failed to load top texture for block '{}'", block.name)
                })?;
                let bottom_image =
                    Self::load_texture(&texture_path.join(bottom)).with_context(|| {
                        format!("Failed to load bottom texture for block '{}'", block.name)
                    })?;
                let side_image =
                    Self::load_texture(&texture_path.join(side)).with_context(|| {
                        format!("Failed to load side texture for block '{}'", block.name)
                    })?;
                BlockTextures::PerFace {
                    top: top_image,
                    bottom: bottom_image,
                    side: side_image,
                }
            }
        };

        let block_entry = BlockDatabaseEntry {
            id: BlockTypeId(block.id),
            name: block.name,
            textures,
            texture_indices: RwLock::new(None),
        };

        self.blocks.push(block_entry);
        Ok(BlockTypeId(block.id))
    }

    fn load_texture(path: &Path) -> anyhow::Result<RgbaImage> {
        let image = image::open(path)
            .with_context(|| format!("Failed to open texture image at {}", path.display()))?;
        Ok(image.to_rgba8())
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
                b.get_texture_indices()
                    .unwrap_or_else(|| TextureIndices::new_single(0))
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
