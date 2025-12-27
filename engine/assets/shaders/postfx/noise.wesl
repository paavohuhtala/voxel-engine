import package::fullscreen_vertex::{FullscreenVertexOutput};

struct PostFxConstants {
    time: f32,
}

@group(0) @binding(0) var screenTexture: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> constants: PostFxConstants;

// Subtle noise / film grain post-process effect, mostly to reduce banding artifacts.
@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let noise_strength: f32 = 0.02;

    let noise_value = rand(in.uv * vec2<f32>(800.0, 600.0) + constants.time * 10.0); 
    let noise_color = vec3<f32>(noise_value, noise_value, noise_value) * noise_strength;

    let original_color = textureSample(screenTexture, samp, in.uv).rgb;
    let final_color = original_color + noise_color;

    return vec4<f32>(final_color, 1.0);
}

fn rand(uv: vec2<f32>) -> f32 {
    let dot_product = dot(uv, vec2<f32>(12.9898, 78.233));
    return fract(sin(dot_product) * 43758.5453);
}
