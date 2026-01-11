use std::array;

use bytemuck::{Pod, Zeroable};
use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use wgpu::{TexelCopyBufferLayout, TexelCopyTextureInfo};

use engine::assets::world_textures::{TextureTransparency, WorldTextures};

use crate::rendering::memory::typed_buffer::GpuBufferArray;

const MAX_TEXTURES: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TextureAttributes(pub u32);

impl TextureAttributes {
    pub fn from_transparency(transparency: TextureTransparency) -> Self {
        TextureAttributes(transparency as u32)
    }
}

pub struct TextureManager {
    queue: wgpu::Queue,
    array_texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    view: wgpu::TextureView,
    texture_attributes: Vec<TextureAttributes>,
    texture_attributes_buffer: GpuBufferArray<TextureAttributes>,
}

pub const TEXTURE_SIZE: usize = 16;
pub const MIP_LAYERS: usize = 1 + TEXTURE_SIZE.ilog2() as usize;
pub const MIP_SIZES: [u32; MIP_LAYERS] = get_mip_sizes();

const fn get_mip_sizes() -> [u32; MIP_LAYERS] {
    let mut sizes = [0u32; MIP_LAYERS];
    let mut level = 0;
    while level < MIP_LAYERS {
        let size = TEXTURE_SIZE >> level;
        sizes[level] = if size == 0 { 1 } else { size as u32 };
        level += 1;
    }
    sizes
}

impl TextureManager {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
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

        let texture_attributes_buffer = GpuBufferArray::new(
            device,
            queue,
            "Texture Attributes Buffer",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            MAX_TEXTURES,
        );

        Self {
            queue: queue.clone(),
            array_texture,
            sampler,
            view,
            texture_attributes: Vec::new(),
            texture_attributes_buffer,
        }
    }

    fn upload_texture(&mut self, texture_index: u16, image: &RgbaImage) -> u16 {
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

            let size = MIP_SIZES[i];
            imageops::resize(image, size, size, FilterType::Triangle)
        })
    }

    fn upload_texture_attributes(&mut self) {
        self.texture_attributes_buffer
            .write_data(&self.texture_attributes);
    }

    pub fn load_all_textures(&mut self, world_textures: &WorldTextures) -> anyhow::Result<()> {
        for (index, texture) in world_textures.textures.iter().enumerate() {
            self.upload_texture(index as u16, &texture.data);
            self.texture_attributes
                .push(TextureAttributes::from_transparency(texture.transparency));
        }

        self.upload_texture_attributes();

        Ok(())
    }

    pub fn array_texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn texture_attributes_buffer(&self) -> &GpuBufferArray<TextureAttributes> {
        &self.texture_attributes_buffer
    }
}
