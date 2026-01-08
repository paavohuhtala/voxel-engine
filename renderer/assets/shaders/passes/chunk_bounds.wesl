import package::common::common::{Camera};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) chunk_position: vec3<i32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Calculate world position: chunk position * 16 + local vertex position
    let chunk_world_pos = vec3<f32>(input.chunk_position) * 16.0;
    let world_pos = chunk_world_pos + input.position;
    
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    
    // Color based on chunk position for variety
    let r = f32((input.chunk_position.x & 3)) / 3.0 * 0.5 + 0.5;
    let g = f32((input.chunk_position.y & 3)) / 3.0 * 0.5 + 0.5;
    let b = f32((input.chunk_position.z & 3)) / 3.0 * 0.5 + 0.5;
    out.color = vec3<f32>(r, g, b);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
