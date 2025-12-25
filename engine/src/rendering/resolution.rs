use winit::dpi::PhysicalSize;

pub type Resolution = PhysicalSize<u32>;

pub trait PhysicalSizeExt {
    fn to_extent3d(&self) -> wgpu::Extent3d;
}

impl PhysicalSizeExt for Resolution {
    fn to_extent3d(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }
}
