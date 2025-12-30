use std::array;

use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use wgpu::{TexelCopyBufferLayout, TexelCopyTextureInfo};

use crate::{
    assets::blocks::{BlockDatabaseEntry, BlockTextures, TextureIndices},
    memory::pool::Pool,
};

const MAX_TEXTURES: usize = 256;

pub struct TextureManager {
    queue: wgpu::Queue,
    texture_pool: Pool,
    array_texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    view: wgpu::TextureView,
}

pub const TEXTURE_SIZE: usize = 16;
pub const MIP_LAYERS: usize = 1 + TEXTURE_SIZE.ilog2() as usize;
pub const MIP_SIZES: [u32; MIP_LAYERS] = get_mip_sizes();

const fn get_mip_sizes() -> [u32; MIP_LAYERS] {
    let mut sizes = [0u32; MIP_LAYERS];
    let mut level = 0;
    while level < MIP_LAYERS {
        let size = TEXTURE_SIZE >> level;
        sizes[level as usize] = if size == 0 { 1 } else { size as u32 };
        level += 1;
    }
    sizes
}

impl TextureManager {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture_pool = Pool::new(MAX_TEXTURES as u64);

        let texture_size = wgpu::Extent3d {
            width: TEXTURE_SIZE as u32,
            height: TEXTURE_SIZE as u32,
            depth_or_array_layers: MAX_TEXTURES as u32,
        };

        let array_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("World texture array"),
            size: texture_size,
            mip_level_count: MIP_LAYERS as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("World texture sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            lod_min_clamp: 0.0,
            lod_max_clamp: MIP_LAYERS as f32,
            ..Default::default()
        });

        let view = array_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("World texture array view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let mut manager = Self {
            queue: queue.clone(),
            texture_pool,
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

    fn load_block_textures(&mut self, block: &BlockDatabaseEntry) -> anyhow::Result<()> {
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

    fn allocate_and_upload_texture(&mut self, image: &RgbaImage) -> u16 {
        let texture_index = self.texture_pool.allocate().expect("Texture pool is full") as u16;
        let mipmaps = self.generate_mipmaps(image);

        for (mip_level, image) in mipmaps.iter().enumerate() {
            let (width, height) = image.dimensions();
            let texture_data = image.as_raw().as_slice();
            // Upload texture data to the array texture
            self.queue.write_texture(
                TexelCopyTextureInfo {
                    aspect: wgpu::TextureAspect::All,
                    mip_level: mip_level as u32,
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
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        texture_index
    }

    fn generate_mipmaps(&self, image: &RgbaImage) -> [RgbaImage; MIP_LAYERS] {
        array::from_fn(|i| {
            if i == 0 {
                return image.clone();
            }

            let size = MIP_SIZES[i as usize];
            imageops::resize(image, size, size, FilterType::Triangle)
        })
    }

    pub fn load_all_textures<'a>(
        &mut self,
        blocks: impl Iterator<Item = &'a BlockDatabaseEntry>,
    ) -> anyhow::Result<()> {
        for block in blocks {
            self.load_block_textures(block)?;
        }

        log::info!(
            "Loaded all block textures. Texture pool at {}/{}",
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
