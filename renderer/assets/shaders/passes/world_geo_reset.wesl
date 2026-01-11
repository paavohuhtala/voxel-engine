
struct DrawIndexedIndirect {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

@group(0) @binding(0)
var<storage, read_write> opaque_draw_commands: array<DrawIndexedIndirect>;
@group(0) @binding(1)
var<storage, read_write> alpha_cutout_draw_commands: array<DrawIndexedIndirect>;

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>
) {
    var index = global_id.x;

    if (index < arrayLength(&opaque_draw_commands)) {
        opaque_draw_commands[index].instance_count = 0u;
    }

    if (index < arrayLength(&alpha_cutout_draw_commands)) {
        alpha_cutout_draw_commands[index].instance_count = 0u;
    }
}
    