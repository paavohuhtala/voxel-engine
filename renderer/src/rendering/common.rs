pub const FULLSCREEN_TRIANGLE_PRIMITIVE_STATE: wgpu::PrimitiveState = wgpu::PrimitiveState {
    topology: wgpu::PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: wgpu::FrontFace::Ccw,
    cull_mode: None,
    unclipped_depth: false,
    polygon_mode: wgpu::PolygonMode::Fill,
    conservative: false,
};

pub struct FullscreenVertexShader {
    module: wgpu::ShaderModule,
}

impl FullscreenVertexShader {
    pub fn new(device: &wgpu::Device) -> Self {
        let source = include_str!(concat!(env!("OUT_DIR"), "/fullscreen_vertex.wgsl"));
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fullscreen triangle vertex shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        FullscreenVertexShader { module }
    }

    pub fn module(&self) -> &wgpu::ShaderModule {
        &self.module
    }

    pub fn vertex_state<'a>(&'a self) -> wgpu::VertexState<'a> {
        wgpu::VertexState {
            module: &self.module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        }
    }
}
