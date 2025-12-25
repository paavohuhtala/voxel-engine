
struct CameraUniform {
    view_proj: mat4x4<f32>
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<storage, read> chunks: array<Chunk>;

struct Chunk {
    position: vec4<i32>,
}

struct VertexInput {
    // Uint8x4 is represented as vec4<u32> in WGSL
    // w contains face index (Top, Bottom, Left, Right, Front, Back)
    @location(0) position: vec4<u32>,
    // Last 16 bits unused
    @location(1) material_index: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) material_index: u32,
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

    var face_id = input.position.w;
    var normal = get_face_normal(input.position.w);
    var uv = get_uv(local_pos, face_id);

    out.position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.normal = normal;
    out.uv = uv;
    out.material_index = input.material_index;

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
    // Draw voxel borders based
    var face_uv = fract(input.uv);
    var border_factor = 0.0;
    if (face_uv.x < 0.05 || face_uv.x > 0.95 || face_uv.y < 0.05 || face_uv.y > 0.95) {
        border_factor = 1.0;
    }
    return vec4<f32>(abs(input.normal) * (1.0 - border_factor) + vec3<f32>(0.0, 0.0, 0.0) * border_factor, 1.0);
}