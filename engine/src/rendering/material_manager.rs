use image::RgbaImage;
use wgpu::{TexelCopyBufferLayout, TexelCopyTextureInfo};

use crate::{
    assets::blocks::{BlockDatabaseEntry, BlockTextures, TextureIndices},
    memory::pool::Pool,
};

const MAX_MATERIALS: usize = 256;

pub struct MaterialManager {
    device: wgpu::Device,
    queue: wgpu::Queue,

    // Store all materials in an array texture
    texture_pool: Pool,
    texture_size: wgpu::Extent3d,
    array_texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    view: wgpu::TextureView,
}

impl MaterialManager {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture_pool = Pool::new(MAX_MATERIALS as u64);

        let texture_size = wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: MAX_MATERIALS as u32,
        };

        let array_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Texture Array"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Material Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let view = array_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Material Texture Array View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let mut manager = Self {
            device: device.clone(),
            queue: queue.clone(),
            texture_pool,
            texture_size,
            array_texture,
            sampler,
            view,
        };
        manager.add_default_texture();
        manager
    }

    fn add_default_texture(&mut self) {
        let invalid_texture = generate_invalid_texture_checkerboard();
        let id = self.allocate_and_upload_texture(&invalid_texture);
        assert!(id == 0, "Default invalid texture must be at index 0");
    }

    fn load_block_materials(&mut self, block: &BlockDatabaseEntry) -> anyhow::Result<()> {
        match block.get_textures() {
            BlockTextures::Invisible => {
                // No texture to load
                return Ok(());
            }
            BlockTextures::Single(texture) => {
                let texture_index = self.allocate_and_upload_texture(texture);
                block.set_texture_indices(TextureIndices::new_single(texture_index));
            }
            BlockTextures::PerFace { top, bottom, side } => {
                let top_index = self.allocate_and_upload_texture(top);
                let bottom_index = self.allocate_and_upload_texture(bottom);
                let side_index = self.allocate_and_upload_texture(side);

                block.set_texture_indices(TextureIndices {
                    top: top_index,
                    bottom: bottom_index,
                    side: side_index,
                });
            }
        }

        Ok(())
    }

    fn allocate_and_upload_texture(&mut self, texture: &RgbaImage) -> u16 {
        let texture_index = self.texture_pool.allocate().expect("Texture pool is full") as u16;
        let texture_data = texture.as_raw().as_slice();

        // Upload texture data to the array texture
        self.queue.write_texture(
            TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: texture_index as u32,
                },
                texture: &self.array_texture,
            },
            texture_data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.texture_size.width),
                rows_per_image: Some(self.texture_size.height),
            },
            wgpu::Extent3d {
                width: self.texture_size.width,
                height: self.texture_size.height,
                depth_or_array_layers: 1,
            },
        );

        texture_index
    }

    pub fn load_all_materials<'a>(
        &mut self,
        blocks: impl Iterator<Item = &'a BlockDatabaseEntry>,
    ) -> anyhow::Result<()> {
        for block in blocks {
            self.load_block_materials(block)?;
        }

        log::info!(
            "Loaded all block materials. Texture pool at {}/{}",
            self.texture_pool.used(),
            self.texture_pool.capacity()
        );

        Ok(())
    }

    pub fn array_texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn texture_capacity(&self) -> u64 {
        self.texture_pool.capacity()
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
