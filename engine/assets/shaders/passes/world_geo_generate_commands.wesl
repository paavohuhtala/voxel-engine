
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
    var aabb = get_chunk_aabb(chunk.position.xyz, 0u, 16u);

    var is_visible = is_inside_frustum(frustum, aabb);
    if !is_visible {
        draw_commands[index] = DrawIndexedIndirect(0u, 0u, 0u, 0i, 0u);
    } else {
        draw_commands[index] = DrawIndexedIndirect(
            chunk.mesh_data_index_count,
            1u,
            chunk.mesh_data_index_offset,
            chunk.mesh_data_vertex_offset,
            chunk_id
        );
    }
}

struct AABB {
    min: vec3<f32>,
    max: vec3<f32>,
}

fn get_chunk_aabb(chunk_pos: vec3<i32>, y_min: u32, y_max: u32) -> AABB {
    let min = chunk_pos * vec3<i32>(16) + vec3<i32>(0, i32(y_min), 0);
    let max = chunk_pos * vec3<i32>(16) + vec3<i32>(16, i32(y_max), 16);
    return AABB(vec3<f32>(min), vec3<f32>(max));
}

fn is_inside_frustum(frustum: Frustum, aabb: AABB) -> bool {
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

