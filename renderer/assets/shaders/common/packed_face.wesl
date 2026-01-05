struct VoxelFace {
    position: vec3<u32>,
    face_id: u32,
    size: vec2<u32>,
    ambient_occlusion: vec4<u32>,
    flip_diagonal: bool,
    texture_index: u32,
}

fn unpack_face(packed: vec2<u32>) -> VoxelFace {
    let first = packed.x;
    let second = packed.y;

    var face: VoxelFace;

    // Position (x, y, z) - 4 bits each
    face.position = vec3<u32>(
        extractBits(first, 0u, 4u),
        extractBits(first, 4u, 4u),
        extractBits(first, 8u, 4u)
    );

    face.face_id = extractBits(first, 12u, 3u);
    face.flip_diagonal = extractBits(first, 15u, 1u) != 0u;

    // Size (width, height) - 4 bits each, stored as value - 1
    face.size = vec2<u32>(
        extractBits(first, 16u, 4u) + 1u,
        extractBits(first, 20u, 4u) + 1u
    );

    // Ambient Occlusion - 2 bits each
    // Order: bottom-left, bottom-right, top-right, top-left
    face.ambient_occlusion = vec4<u32>(
        extractBits(first, 24u, 2u),
        extractBits(first, 26u, 2u),
        extractBits(first, 28u, 2u),
        extractBits(first, 30u, 2u)
    );

    face.texture_index = extractBits(second, 0u, 16u);

    return face;
}
