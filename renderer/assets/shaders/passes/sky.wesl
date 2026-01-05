
import package::common::common::{Camera};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct FullscreenVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) view_dir: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> FullscreenVertexOutput {
    var out: FullscreenVertexOutput;

    let x = f32((vertex_index & 1u) << 2u) - 1.0;
    let y = f32((vertex_index & 2u) << 1u) - 1.0;

    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, y * 0.5 + 0.5);

    // Reconstruct view ray at far plane (z = 0.0 for reverse-Z)
    let world_pos = camera.view_proj_inverse * out.clip_position;
    let world_dir = normalize(world_pos.xyz / world_pos.w - camera.camera_position.xyz);
    out.view_dir = world_dir;

    return out;
}

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.view_dir);
    
    let color_top = vec3<f32>(0.1, 0.4, 0.8);    // Deep Sky Blue
    let color_horizon = vec3<f32>(0.24, 0.67, 1); // Hazy White/Blue
    let color_bottom = vec3<f32>(0.1, 0.1, 0.1); // Dark ground
    let sun_color = vec3<f32>(1.0, 0.9, 0.6);    // Yellowish Sun

    // --- Simple Gradient ---
    // Use the Y component of the direction for the gradient (0.0 is horizon, 1.0 is straight up)
    var sky_color = mix(color_horizon, color_top, pow(max(dir.y, 0.0), 0.8));
    
    // Mix with ground color if looking down
    sky_color = mix(sky_color, color_bottom, clamp(-dir.y * 3.0, 0.0, 1.0));

    // --- Procedural Sun ---
    // Dot product tells us how close we are looking at the sun
    let sun_dot = max(dot(dir, normalize(camera.sun_direction.xyz)), 0.0);
    
    // Sun disc (hard edge or slightly soft)
    let sun_radius = 0.998; // Closer to 1.0 = smaller sun
    let sun_intensity = smoothstep(sun_radius, sun_radius + 0.001, sun_dot);
    
    // Sun Halo (glow around the sun)
    let sun_halo = pow(sun_dot, 200.0) * 0.5;

    // Combine
    let final_color = sky_color + (sun_color * sun_intensity) + (sun_color * sun_halo);

    return vec4<f32>(final_color, 1.0);
}