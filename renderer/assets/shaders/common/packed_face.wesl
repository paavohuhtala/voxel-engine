struct VoxelFace {
    position: vec3<u32>,
    face_id: u32,
    size: vec2<u32>,
    ambient_occlusion: vec4<u32>,
    flip_diagonal: bool,
    texture_index: u32,
}

// Packed face layout (6 bytes = 48 bits):
//
// Bytes 0-3: Geometry data (32 bits)
//   bits 0-3:   position.x (4 bits)
//   bits 4-7:   position.y (4 bits)
//   bits 8-11:  position.z (4 bits)
//   bits 12-14: face_id (3 bits)
//   bit 15:     flip_diagonal (1 bit)
//   bits 16-19: size.x - 1 (4 bits)
//   bits 20-23: size.y - 1 (4 bits)
//   bits 24-31: ambient_occlusion (2 bits each, 4 corners)
//
// Bytes 4-5: Texture index (16 bits)

fn unpack_face(geometry: u32, texture_index: u32) -> VoxelFace {
    var face: VoxelFace;

    // Position (x, y, z) - 4 bits each
    face.position = vec3<u32>(
        extractBits(geometry, 0u, 4u),
        extractBits(geometry, 4u, 4u),
        extractBits(geometry, 8u, 4u)
    );

    face.face_id = extractBits(geometry, 12u, 3u);
    face.flip_diagonal = extractBits(geometry, 15u, 1u) != 0u;

    // Size (width, height) - 4 bits each, stored as value - 1
    face.size = vec2<u32>(
        extractBits(geometry, 16u, 4u) + 1u,
        extractBits(geometry, 20u, 4u) + 1u
    );

    // Ambient Occlusion - 2 bits each
    // Order: bottom-left, bottom-right, top-right, top-left
    face.ambient_occlusion = vec4<u32>(
        extractBits(geometry, 24u, 2u),
        extractBits(geometry, 26u, 2u),
        extractBits(geometry, 28u, 2u),
        extractBits(geometry, 30u, 2u)
    );

    face.texture_index = texture_index;

    return face;
}
