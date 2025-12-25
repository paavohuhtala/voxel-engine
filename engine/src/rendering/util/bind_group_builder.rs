pub struct BindGroupBuilder<'a> {
    name: String,
    visibility: wgpu::ShaderStages,
    bindings: Vec<BindingConfig<'a>>,
}

struct BindingConfig<'a> {
    index: u32,
    _name: String,
    binding_type: BindingConfigType,
    count: Option<std::num::NonZeroU32>,
    resource: wgpu::BindingResource<'a>,
}

enum BindingConfigType {
    // This could be extended to support more types
    Buffer(wgpu::BufferBindingType),
}

impl<'a> BindGroupBuilder<'a> {
    pub fn new(name: impl Into<String>, visibility: wgpu::ShaderStages) -> Self {
        BindGroupBuilder {
            name: name.into(),
            visibility,
            bindings: Vec::new(),
        }
    }

    pub fn uniform(
        mut self,
        index: u32,
        name: impl Into<String>,
        resource: wgpu::BindingResource<'a>,
    ) -> Self {
        self.bindings.push(BindingConfig {
            index,
            _name: name.into(),
            binding_type: BindingConfigType::Buffer(wgpu::BufferBindingType::Uniform),
            count: None,
            resource,
        });
        self
    }

    pub fn storage_r(
        mut self,
        index: u32,
        name: impl Into<String>,
        resource: wgpu::BindingResource<'a>,
    ) -> Self {
        self.bindings.push(BindingConfig {
            index,
            _name: name.into(),
            binding_type: BindingConfigType::Buffer(wgpu::BufferBindingType::Storage {
                read_only: true,
            }),
            count: None,
            resource,
        });
        self
    }

    pub fn storage_rw(
        mut self,
        index: u32,
        name: impl Into<String>,
        resource: wgpu::BindingResource<'a>,
    ) -> Self {
        self.bindings.push(BindingConfig {
            index,
            _name: name.into(),
            binding_type: BindingConfigType::Buffer(wgpu::BufferBindingType::Storage {
                read_only: false,
            }),
            count: None,
            resource,
        });
        self
    }

    pub fn build(self, device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let layout_entries: Vec<wgpu::BindGroupLayoutEntry> = self
            .bindings
            .iter()
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding: binding.index,
                visibility: self.visibility,
                ty: match &binding.binding_type {
                    BindingConfigType::Buffer(buffer_type) => wgpu::BindingType::Buffer {
                        ty: *buffer_type,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                count: binding.count,
            })
            .collect();

        let layout_label = format!("{} bind group layout", self.name);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&layout_label),
            entries: &layout_entries,
        });

        let bind_entries = self
            .bindings
            .into_iter()
            .map(|binding| wgpu::BindGroupEntry {
                binding: binding.index,
                resource: binding.resource,
            })
            .collect::<Vec<_>>();

        let group_label = format!("{} bind group", self.name);

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&group_label),
            layout: &layout,
            entries: &bind_entries,
        });

        (layout, group)
    }
}
