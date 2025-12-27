
struct Chunk {
    position: vec4<i32>,
    mesh_data_index_offset: u32,
    mesh_data_index_count: u32,
    mesh_data_vertex_offset: i32,
}

struct Camera {
    view_proj: mat4x4<f32>,
    view_proj_inverse: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
}
