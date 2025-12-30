
import package::common::common::{Chunk, Camera};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> chunks: array<Chunk>;

// Textures and sampler
@group(2) @binding(0)
var textures: texture_2d_array<f32>;
@group(2) @binding(1)
var array_sampler: sampler;

struct VertexInput {
    // Uint8x4 is represented as vec4<u32> in WGSL
    // w contains face index (Top, Bottom, Left, Right, Front, Back)
    @location(0) position: vec4<u32>,
    @location(1) texture_index_and_ao: vec2<u32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) texture_index: u32,
    @location(3) light_factor: f32,
    @location(4) ambient_occlusion: f32,
}

@vertex
fn vs_main(input: VertexInput, @builtin(instance_index) instance_idx: u32) -> VertexOutput {
    var chunk = chunks[instance_idx];

    var out: VertexOutput;
    let local_pos = input.position.xyz;
    let world_pos = vec3<f32>(
        f32(chunk.position.x * 16) + f32(local_pos.x),
        f32(chunk.position.y * 16) + f32(local_pos.y),
        f32(chunk.position.z * 16) + f32(local_pos.z),
    );

    let face_id = input.position.w;
    let normal = get_face_normal(input.position.w);
    let uv = get_uv(local_pos, face_id);

    out.position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.uv = uv;

    let texture_index = input.texture_index_and_ao.x;
    let ambient_occlusion = input.texture_index_and_ao.y;
    out.texture_index = texture_index;
    // Normalize ambient occlusion to [0.0, 1.0]
    out.ambient_occlusion = f32(ambient_occlusion) / 2.0;

    // HACK: AO enabled / disabled is passed via camera.sun_direction.w
    if (!bool(camera.sun_direction.w)) {
        out.ambient_occlusion = 0.0;
    }

    // Calculate basic lighting from normal and camera.sun_direction
    let light_dir = normalize(camera.sun_direction.xyz);
    let light_intensity = max(dot(normal, light_dir), 0.1);
    out.light_factor = light_intensity;

    return out;
}

fn get_face_normal(face_idx: u32) -> vec3<f32> {
    switch (face_idx) {
        case 0: { return vec3<f32>(0.0, 1.0, 0.0); } // Top
        case 1: { return vec3<f32>(0.0, -1.0, 0.0); } // Bottom
        case 2: { return vec3<f32>(-1.0, 0.0, 0.0); } // Left
        case 3: { return vec3<f32>(1.0, 0.0, 0.0); } // Right
        case 4: { return vec3<f32>(0.0, 0.0, 1.0); } // Front
        case 5: { return vec3<f32>(0.0, 0.0, -1.0); } // Back
        default: { return vec3<f32>(0.0, 0.0, 0.0); } // Should not happen
    }
}

fn get_uv(local_pos: vec3<u32>, face_idx: u32) -> vec2<f32> {
    switch (face_idx) {
        case 0: { return vec2<f32>(f32(local_pos.x), f32(local_pos.z)); } // Top
        case 1: { return vec2<f32>(-f32(local_pos.x), f32(local_pos.z)); } // Bottom
        case 2: { return vec2<f32>(f32(local_pos.z), f32(local_pos.y)); } // Left
        case 3: { return vec2<f32>(-f32(local_pos.z), f32(local_pos.y)); } // Right
        case 4: { return vec2<f32>(f32(local_pos.x), f32(local_pos.y)); } // Front
        case 5: { return vec2<f32>(-f32(local_pos.x), f32(local_pos.y)); } // Back
        default: { return vec2<f32>(0.0, 0.0); } // Should not happen
    }
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uv;
    var border_factor = 0.0;
    if (uv.x < 0.05 || uv.x > 0.95 || uv.y < 0.05 || uv.y > 0.95) {
        border_factor = 1.0;
    }

    let ambient_occlusion = clamp(pow(input.ambient_occlusion, 2.0), 0.0, 1.0);
    let ao_factor = mix(1.0, 0.1, ambient_occlusion);

    let texture_color = textureSampleSharp(uv, input.texture_index).rgb * input.light_factor * ao_factor;
    // TODO: borders currently do nothing, add render settings uniform and allow toggling borders
    let final_color = mix(texture_color, texture_color, border_factor);
    return vec4<f32>(final_color, 1.0);
}

// Samples a texture with pixel-perfect results, while maintaining correct derivatives for mipmapping / anisotropic filtering
fn textureSampleSharp(uv: vec2<f32>, texture_index: u32) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(textures));
    let ddx = dpdx(uv);
    let ddy = dpdy(uv);
    let pixel_coord = uv * size;
    let seam = floor(pixel_coord + 0.5);
    let dist = pixel_coord - seam;
    let derivative_len = fwidth(pixel_coord);
    let sharp_dist = clamp(
        dist / max(derivative_len, vec2<f32>(0.0001)), 
        vec2<f32>(-0.5), 
        vec2<f32>(0.5)
    );
    let uv_sharp = (seam + sharp_dist) / size;
    return textureSampleGrad(textures, array_sampler, uv_sharp, texture_index, ddx, ddy);
}
