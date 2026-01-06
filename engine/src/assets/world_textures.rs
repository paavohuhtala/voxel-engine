use std::path::{Path, PathBuf};

use anyhow::Context;
use image::RgbaImage;

#[derive(Debug, Clone, Copy)]
pub struct WorldTextureHandle(pub u16);

impl WorldTextureHandle {
    pub const ERROR: Self = WorldTextureHandle(0);
}

pub struct WorldTextures {
    // TODO: Do we have to keep textures in both RAM and VRAM?
    // I guess with Minecraft-style textures that doesn't really matter
    pub textures: Vec<RgbaImage>,
    base_path: std::path::PathBuf,
}

impl WorldTextures {
    pub fn new() -> Self {
        let mut world_textures = WorldTextures {
            textures: Vec::new(),
            base_path: PathBuf::from("assets").join("textures"),
        };

        let invalid_texture = generate_invalid_texture_checkerboard();
        world_textures.allocate(invalid_texture);
        world_textures
    }

    fn allocate(&mut self, texture: RgbaImage) -> WorldTextureHandle {
        let index = self.textures.len();
        self.textures.push(texture);
        WorldTextureHandle(index as u16)
    }

    // TODO: This could be done asynchronously
    pub fn load_from_path_and_allocate(
        &mut self,
        path: &str,
    ) -> anyhow::Result<WorldTextureHandle> {
        let path = self.base_path.join(path);
        let texture = Self::load_texture(&path)?;
        Ok(self.allocate(texture))
    }

    fn load_texture(path: &Path) -> anyhow::Result<RgbaImage> {
        let image = image::open(path)
            .with_context(|| format!("Failed to open texture image at {}", path.display()))?;
        Ok(image.to_rgba8())
    }
}

impl Default for WorldTextures {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_invalid_texture_checkerboard() -> RgbaImage {
    const SIZE: u32 = 16;
    let mut img = RgbaImage::new(SIZE, SIZE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let is_white = ((x / 4) + (y / 4)) % 2 == 0;
            let color = if is_white {
                [255, 0, 255, 255] // Magenta
            } else {
                [0, 0, 0, 255] // Black
            };
            img.put_pixel(x, y, image::Rgba(color));
        }
    }
    img
}
