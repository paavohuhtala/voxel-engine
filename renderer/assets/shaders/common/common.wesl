
struct Chunk {
    position: vec4<i32>,
    face_byte_offset: u32,
    face_count: u32,
    aabb: u32,
    padding: u32,
}

struct Camera {
    view_proj: mat4x4<f32>,
    view_proj_inverse: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    flags: vec4<u32>,
}
