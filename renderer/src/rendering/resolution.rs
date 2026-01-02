use winit::dpi::PhysicalSize;

pub type Resolution = PhysicalSize<u32>;

pub trait PhysicalSizeExt {
    fn to_extent3d(&self) -> wgpu::Extent3d;
    fn to_vec2(&self) -> glam::Vec2;
}

impl PhysicalSizeExt for Resolution {
    fn to_extent3d(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }

    fn to_vec2(&self) -> glam::Vec2 {
        glam::Vec2::new(self.width as f32, self.height as f32)
    }
}
