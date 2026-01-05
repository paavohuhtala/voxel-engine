
import package::common::common::{Chunk, Camera};

struct Frustum {
    planes: array<vec4<f32>, 6>,
}

struct CullingParams {
    frustum: Frustum,
    input_chunk_count: u32,
}

struct DrawIndexedIndirect {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<storage, read> chunks: array<Chunk>;

@group(2) @binding(0)
var<uniform> culling_params: CullingParams;

@group(2) @binding(1)
var<storage, read> input_chunk_ids: array<u32>;

@group(2) @binding(2)
var<storage, read_write> draw_commands: array<DrawIndexedIndirect>;

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>
) {
    var index = global_id.x;

    if (index >= culling_params.input_chunk_count) {
        return;
    }
    var chunk_id = input_chunk_ids[index];
    var chunk = chunks[chunk_id];
    var frustum = culling_params.frustum;
    var aabb = unpack_aabb(chunk.aabb);

    let chunk_pos = vec3<f32>(f32(chunk.position.x), f32(chunk.position.y), f32(chunk.position.z)) * 16.0;
    aabb.min += chunk_pos;
    aabb.max += chunk_pos;

    var is_visible = intersects_frustum(frustum, aabb);
    if !is_visible {
        draw_commands[index] = DrawIndexedIndirect(0u, 0u, 0u, 0i, 0u);
    } else {
        // Let's abuse base_vertex to smuggle chunk_id to the vertex shader!
        // base_vertex is added to the index and passed to the vertex shader as vertex_index
        // Since each face has 4 vertices, the indices range between 0 and 3. We only need the two lowest bits for the actual vertex index.
        // The vertex shader undoes this packing to get the chunk id back.
        let packed_chunk_id = bitcast<i32>(insertBits(0u, chunk_id, 2u, 30u));

        draw_commands[index] = DrawIndexedIndirect(
            6,
            chunk.face_count,
            0,
            packed_chunk_id,
            chunk.face_data_offset
        );
    }
}

struct AABB {
    min: vec3<f32>,
    max: vec3<f32>,
}

fn unpack_aabb(packed: u32) -> AABB {
    let min_x = f32(extractBits(packed, 0u, 4u));
    let min_y = f32(extractBits(packed, 4u, 4u));
    let min_z = f32(extractBits(packed, 8u, 4u));
    let max_x = f32(extractBits(packed, 12u, 4u)) + 1.0;
    let max_y = f32(extractBits(packed, 16u, 4u)) + 1.0;
    let max_z = f32(extractBits(packed, 20u, 4u)) + 1.0;
    return AABB(
        vec3<f32>(min_x, min_y, min_z),
        vec3<f32>(max_x, max_y, max_z)
    );
}

fn intersects_frustum(frustum: Frustum, aabb: AABB) -> bool {
    for (var i: u32 = 0u; i < 6u; i = i + 1u) {
        let plane = frustum.planes[i];
        let p_vertex = vec3<f32>(
            select(aabb.min.x, aabb.max.x, plane.x >= 0.0),
            select(aabb.min.y, aabb.max.y, plane.y >= 0.0),
            select(aabb.min.z, aabb.max.z, plane.z >= 0.0)
        );

        let distance = dot(plane.xyz, p_vertex) + plane.w;
        if (distance < 0.0) {
            return false;
        }
    }
    return true;
}

