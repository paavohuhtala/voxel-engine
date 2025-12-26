
import package::common::chunk::Chunk;

struct CullingParams {
    input_chunk_count: u32,
}

struct CameraUniform {
    view_proj: mat4x4<f32>
}

struct DrawIndexedIndirect {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<storage, read> chunks: array<Chunk>;

@group(2) @binding(0)
var<uniform> culling_params: CullingParams;

@group(2) @binding(1)
var<storage, read> input_chunk_ids: array<u32>;

@group(2) @binding(2)
var<storage, read_write> draw_commands: array<DrawIndexedIndirect>;

@group(2) @binding(3)
var<storage, read_write> draw_count: atomic<u32>;

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

    // TODO: Frustum culling logic goes here
    var is_visible = true;
    if !is_visible {
        return;
    }

    var out_index = atomicAdd(&draw_count, 1u);

    draw_commands[out_index] = DrawIndexedIndirect(
        chunk.mesh_data_index_count,
        1u,
        chunk.mesh_data_index_offset,
        chunk.mesh_data_vertex_offset,
        chunk_id
    );
}
