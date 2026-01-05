
import package::common::common::{Chunk, Camera};
import package::common::packed_face::{VoxelFace, unpack_face};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> chunks: array<Chunk>;
@group(1) @binding(1)
var<storage, read> faces_raw: array<u32>;

@group(2) @binding(0)
var textures: texture_2d_array<f32>;
@group(2) @binding(1)
var array_sampler: sampler;

// Read a 6-byte packed face from the faces buffer given a byte offset.
// Each face is 6 bytes: 4 bytes geometry + 2 bytes texture index.
// We read from the u32 array and reconstruct the face data.
fn read_face_at_byte_offset(byte_offset: u32) -> VoxelFace {
    // Calculate u32 index and position within that u32
    let u32_index = byte_offset / 4u;
    let byte_in_u32 = byte_offset % 4u;

    var geometry: u32;
    var texture_index: u32;

    // Depending on alignment, the face can start at byte 0 or 2 within a u32
    if (byte_in_u32 == 0u) {
        // Face starts at u32 boundary
        geometry = faces_raw[u32_index];
        texture_index = faces_raw[u32_index + 1u] & 0xFFFFu;
    } else if (byte_in_u32 == 2u) {
        // Face starts at byte 2
        let word0 = faces_raw[u32_index];
        let word1 = faces_raw[u32_index + 1u];
        geometry = (word0 >> 16u) | (word1 << 16u);
        texture_index = word1 >> 16u;
    } else {
        // byte_in_u32 == 1 or 3: shouldn't happen with 4-byte aligned allocations
        return unpack_face(0u, 0u);
    }

    return unpack_face(geometry, texture_index);
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) texture_index: u32,
    @location(3) light_factor: f32,
    @location(4) ambient_occlusion: f32,
    @location(5) face_id: u32,
    @location(6) show_face_colors: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) index_and_chunk_index: u32,
    @builtin(instance_index) local_face_index: u32
) -> VertexOutput {
    // Decode chunk index and vertex index
    let vertex_index = extractBits(index_and_chunk_index, 0u, 2u);
    let chunk_index = extractBits(index_and_chunk_index, 2u, 30u);

    let chunk = chunks[chunk_index];
    let chunk_origin = vec3<f32>(chunk.position.xyz) * 16.0;
    
    // Calculate byte offset for this face
    // local_face_index is 0, 1, 2, ... for faces within this chunk
    // chunk.face_byte_offset is the byte offset of the first face
    let face_byte_offset = chunk.face_byte_offset + local_face_index * 6u;
    let face = read_face_at_byte_offset(face_byte_offset);

    var vertex_data = get_vertex(
        vertex_index,
        face,
        chunk_origin
    );

    var out: VertexOutput;
    out.position = camera.view_proj * vec4<f32>(vertex_data.position, 1.0);
    out.normal = vertex_data.normal;
    out.uv = vertex_data.uv;

    let texture_index = face.texture_index;
    out.texture_index = texture_index;
    out.ambient_occlusion = select(0.0, vertex_data.ambient_occlusion, bool(camera.flags.x & 0x1u));

    // Calculate basic lighting from normal and camera.sun_direction
    let light_dir = normalize(camera.sun_direction.xyz);
    let light_intensity = max(dot(vertex_data.normal, light_dir), 0.1);
    out.light_factor = light_intensity;

    out.face_id = face.face_id;
    out.show_face_colors = camera.flags.x & 0x2u;

    return out;
}

// All following tables are in the our standard face order:
// Top (Y+), Bottom (Y-), Left (X-), Right (X+), Front (Z+), Back (Z-)

var<private> NORMALS: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, -1.0, 0.0),
    vec3<f32>(-1.0, 0.0, 0.0),
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(0.0, 0.0, -1.0),
);

var<private> TANGENTS: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(1.0, 0.0, 0.0),
);

var<private> BITANGENTS: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
);

var<private> SWAP_WINDING: array<bool, 6> = array<bool, 6>(
    true,
    false,
    true,
    false,
    true,
    false
);

var<private> QUAD_CORNERS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0), // Bottom-Left
    vec2<f32>(1.0, 0.0), // Bottom-Right
    vec2<f32>(1.0, 1.0), // Top-Right
    vec2<f32>(0.0, 1.0), // Top-Left
);

struct VertexData {
    position: vec3<f32>,
    uv: vec2<f32>,
    normal: vec3<f32>,
    ambient_occlusion: f32,
}

fn get_vertex(
    vertex_index: u32,
    face: VoxelFace,
    chunk_origin: vec3<f32>
) -> VertexData {
    let normal = NORMALS[face.face_id];
    let tangent = TANGENTS[face.face_id];
    let bitangent = BITANGENTS[face.face_id];

    // If diagonal flip is set, rotate corners by 1
    let base_index = (vertex_index + u32(face.flip_diagonal)) % 4u;

    // Swap winding order for certain faces to get the correct triangle order
    var corner_index = base_index;
    if (SWAP_WINDING[face.face_id]) {
        if (base_index == 1u) { corner_index = 3u; }
        else if (base_index == 3u) { corner_index = 1u; }
    }

    // Calculate final vertex position
    let corner_uv = QUAD_CORNERS[corner_index];
    let face_offset = max(normal, vec3<f32>(0.0));
    let local_pos = (tangent * (corner_uv.x * f32(face.size.x))) +
                    (bitangent * (corner_uv.y * f32(face.size.y)));
    let final_pos = chunk_origin + vec3<f32>(face.position) + face_offset + local_pos;

    // Calculate texture UVs
    let uv = vec2<f32>(
        corner_uv.x * f32(face.size.x),
        // Y axis is flipped in texture space
        f32(face.size.y) - (corner_uv.y * f32(face.size.y))
    );

    // Calculate ambient occlusion
    let int_ao = face.ambient_occlusion[corner_index];
    let ao = VERTEX_AO_TO_FACTOR[int_ao];

    return VertexData(final_pos, uv, normal, ao);
}

var<private> VERTEX_AO_TO_FACTOR: array<f32, 4> = array<f32, 4>(
   0.0,
   0.5,
   0.75,
   1.0
);

var<private> debug_face_colors: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>(1.0, 0.0, 0.0), // Top - Red
    vec3<f32>(0.0, 1.0, 0.0), // Bottom - Green
    vec3<f32>(0.0, 0.0, 1.0), // Left - Blue
    vec3<f32>(1.0, 1.0, 0.0), // Right - Yellow
    vec3<f32>(1.0, 0.0, 1.0), // Front - Magenta
    vec3<f32>(0.0, 1.0, 1.0), // Back - Cyan
);

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uv;
    var border_factor = 0.0;
    if (uv.x < 0.05 || uv.x > 0.95 || uv.y < 0.05 || uv.y > 0.95) {
        border_factor = 1.0;
    }

    let ambient_occlusion = clamp(pow(input.ambient_occlusion, 2.0), 0.0, 1.0);
    let ao_factor = mix(1.0, 0.2, ambient_occlusion);

    let debug_color = debug_face_colors[input.face_id];
    let texture_color = textureSampleSharp(uv, input.texture_index).rgb;
    let primary_color = select(
        texture_color * input.light_factor,
        debug_color,
        input.show_face_colors > 0u
    );

    let with_lighting = primary_color * ao_factor;
    // TODO: borders currently do nothing, add render settings uniform and allow toggling borders
    let final_color = mix(with_lighting, with_lighting, border_factor);
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
