use winit::dpi::PhysicalSize;

use crate::rendering::resolution::Resolution;

pub struct Texture {
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    descriptor: wgpu::TextureDescriptor<'static>,
    _sampler: wgpu::Sampler,
}

impl Texture {
    #[allow(dead_code)]
    pub fn from_descriptor(
        device: &wgpu::Device,
        descriptor: wgpu::TextureDescriptor<'static>,
        sampler: Option<wgpu::Sampler>,
    ) -> Self {
        let texture = device.create_texture(&descriptor);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = sampler.unwrap_or_else(|| Self::default_sampler(device));

        Self {
            texture,
            view,
            descriptor,
            _sampler: sampler,
        }
    }

    fn default_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        })
    }

    pub fn from_wgpu_texture(
        device: &wgpu::Device,
        descriptor: wgpu::TextureDescriptor<'static>,
        texture: wgpu::Texture,
        sampler: Option<wgpu::Sampler>,
    ) -> Self {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = sampler.unwrap_or_else(|| Self::default_sampler(device));

        Self {
            texture,
            view,
            descriptor,
            _sampler: sampler,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        let new_descriptor = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            ..self.descriptor
        };

        let texture = device.create_texture(&new_descriptor);
        self.texture = texture;
        self.view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
    }
}

pub struct DepthTexture(Texture);

impl DepthTexture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(device: &wgpu::Device, size: Resolution, label: &'static str) -> Self {
        let (descriptor, texture) = Self::create_texture(device, size, label);
        let texture = Texture::from_wgpu_texture(device, descriptor, texture, None);

        DepthTexture(texture)
    }

    fn create_texture(
        device: &wgpu::Device,
        size: Resolution,
        label: &'static str,
    ) -> (wgpu::TextureDescriptor<'static>, wgpu::Texture) {
        let size = wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        };

        let descriptor = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let wgpu_texture = device.create_texture(&descriptor);

        (descriptor, wgpu_texture)
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        self.0.resize(device, size);
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.0.view
    }
}
