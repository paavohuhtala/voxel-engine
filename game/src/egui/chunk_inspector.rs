use egui::CornerRadius;
use engine::{
    camera::Camera,
    limits::VIEW_DISTANCE,
    math::aabb::AABB,
    voxels::{
        chunk::{CHUNK_SIZE, ChunkData, ChunkState, IChunkRenderState},
        coord::ChunkPos,
        voxel::Voxel,
    },
};
use glam::{IVec3, Vec3, Vec4};
use renderer::{
    renderer_types::{RenderChunk, RenderWorld},
    rendering::{resolution::Resolution, world_renderer::WorldRenderer},
};

pub struct ChunkInspectorState {
    pub enabled: bool,
    pub selected: Option<ChunkPos>,
    pub log_on_pick: bool,
    pub skip_empty_chunks: bool,
    pub use_mesh_aabb: bool,
}

impl Default for ChunkInspectorState {
    fn default() -> Self {
        Self {
            enabled: false,
            selected: None,
            log_on_pick: false,
            skip_empty_chunks: true,
            use_mesh_aabb: false,
        }
    }
}

pub fn draw_chunk_inspector_ui(
    state: &mut ChunkInspectorState,
    world_renderer: &mut WorldRenderer,
    world: &RenderWorld,
    context: &egui::Context,
    resolution: Resolution,
) {
    // Use the same toggle for both picking (AABB intersection) and debug bounds rendering.
    world_renderer.use_mesh_aabb_for_bounds = state.use_mesh_aabb;

    if state.enabled
        && context.input(|i| i.pointer.primary_clicked())
        && !context.is_pointer_over_egui()
        && let Some(pos) = context.input(|i| i.pointer.interact_pos())
    {
        let pixels_per_point = context.pixels_per_point();
        let camera = &world_renderer.camera.interpolated_camera;
        let pick_settings = PickSettings {
            max_distance: (VIEW_DISTANCE as f32 + 8.0) * CHUNK_SIZE as f32,
            skip_empty_chunks: state.skip_empty_chunks,
            use_mesh_aabb: state.use_mesh_aabb,
        };

        if let Some(hit) = pick_chunk_under_cursor(
            world,
            camera,
            resolution,
            pos,
            pixels_per_point,
            &pick_settings,
        ) {
            state.selected = Some(hit);

            if state.log_on_pick {
                log_selected_chunk(world, hit);
            }
        }
    }

    egui::Window::new("Chunk inspector").show(context, |ui| {
        ui.style_mut().visuals.window_corner_radius = CornerRadius::ZERO;

        ui.horizontal(|ui| {
            ui.checkbox(&mut state.enabled, "Enable");
            ui.checkbox(&mut state.log_on_pick, "Log on pick");
            ui.checkbox(&mut state.skip_empty_chunks, "Skip empty");
            ui.checkbox(&mut state.use_mesh_aabb, "Use mesh AABB");
        });

        ui.label("Click a chunk in the 3D view to inspect it.");

        let Some(selected) = state.selected else {
            ui.separator();
            ui.label("Selected: (none)");
            return;
        };

        ui.separator();
        ui.label(format!("Selected: {:?}", selected));

        let Some(chunk) = world.chunks.get(&selected) else {
            ui.colored_label(egui::Color32::YELLOW, "Chunk not present in world map");
            return;
        };

        let state_val = chunk.state.load();
        ui.label(format!("State: {:?}", state_val));
        ui.label(format!("Has data: {}", chunk.data.is_some()));

        let neighbor_mask = chunk.neighbor_state.load();
        ui.label(format!("Neighbor mask: {:#08b}", neighbor_mask));

        match chunk.data.as_ref() {
            None => {}
            Some(ChunkData::Solid(v)) => {
                ui.label(format!("Data: Solid (block_type={})", v.block_type()));
            }
            Some(ChunkData::Packed(p)) => {
                ui.label(format!(
                    "Data: Packed (bits_per_voxel={}, palette_len={})",
                    p.bits_per_voxel,
                    p.palette.voxel_types.len()
                ));
            }
        }

        if let Some(render_state) = chunk.render_state.as_ref() {
            ui.separator();
            ui.label("Render:");
            ui.label(format!("GPU chunk id: {}", render_state.chunk_gpu_id()));
            ui.label(format!(
                "Faces allocation: {} bytes",
                render_state.mesh.faces_handle.size_bytes
            ));

            // Show which AABB the picker will use.
            let (pick_label, pick_aabb) =
                chunk_pick_aabb(selected, chunk.value(), state.use_mesh_aabb);
            ui.separator();
            ui.label(format!("Pick AABB: {}", pick_label));
            ui.label(format!("AABB min: {:?}", pick_aabb.min));
            ui.label(format!("AABB max: {:?}", pick_aabb.max));
        }
    });
}

fn log_selected_chunk(world: &RenderWorld, pos: ChunkPos) {
    let Some(chunk) = world.chunks.get(&pos) else {
        log::info!("{:?}: not present", pos);
        return;
    };

    let mut msg = format!(
        "{:?}: state={:?}, data={}, neighbor_mask={:#08b}",
        pos,
        chunk.state.load(),
        chunk.data.is_some(),
        chunk.neighbor_state.load()
    );

    if let Some(render_state) = chunk.render_state.as_ref() {
        msg.push_str(&format!(
            ", gpu_id={}, faces_bytes={}",
            render_state.chunk_gpu_id(),
            render_state.mesh.faces_handle.size_bytes
        ));
    }

    log::info!("{}", msg);
}

struct PickSettings {
    max_distance: f32,
    skip_empty_chunks: bool,
    use_mesh_aabb: bool,
}

fn pick_chunk_under_cursor(
    world: &RenderWorld,
    camera: &Camera,
    resolution: Resolution,
    cursor_pos: egui::Pos2,
    pixels_per_point: f32,
    settings: &PickSettings,
) -> Option<ChunkPos> {
    let (origin, dir) = screen_ray(camera, resolution, cursor_pos, pixels_per_point)?;

    pick_first_existing_chunk(world, origin, dir, settings)
        .or_else(|| pick_by_aabb_intersection(world, origin, dir, settings))
}

fn chunk_pick_aabb(
    pos: ChunkPos,
    chunk: &RenderChunk,
    use_mesh_aabb: bool,
) -> (&'static str, AABB) {
    let origin = pos.origin().0.as_vec3();

    if use_mesh_aabb && let Some(render_state) = chunk.render_state.as_ref() {
        // Mesh AABB is in chunk-local voxel coordinates (u8 in [0,16)).
        // `compute_aabb` stores max as the maximum occupied voxel coordinate (inclusive).
        // Convert to world-space by expanding max by +1 voxel.
        let local = render_state.mesh.aabb;
        let min = origin + local.min.as_vec3();
        let max = origin + local.max.as_vec3() + Vec3::ONE;
        return ("mesh", AABB::new(min, max));
    }

    (
        "chunk",
        AABB::new(origin, origin + Vec3::splat(CHUNK_SIZE as f32)),
    )
}

fn screen_ray(
    camera: &Camera,
    resolution: Resolution,
    cursor_pos: egui::Pos2,
    pixels_per_point: f32,
) -> Option<(Vec3, Vec3)> {
    let w = resolution.width as f32;
    let h = resolution.height as f32;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    let x_px = (cursor_pos.x * pixels_per_point).clamp(0.0, w);
    let y_px = (cursor_pos.y * pixels_per_point).clamp(0.0, h);

    let ndc_x = (x_px / w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y_px / h) * 2.0;

    let inv_vp = camera.view_projection_inverse_matrix;
    let p_near = inv_vp * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

    if p_near.w.abs() < f32::EPSILON {
        return None;
    }
    let near = p_near.truncate() / p_near.w;

    // Build ray from camera eye through the near point
    let origin = camera.eye;
    let dir = (near - origin).normalize_or_zero();
    if dir.length_squared() == 0.0 {
        return None;
    }

    Some((origin, dir))
}

fn is_pickable_render_chunk(chunk: &RenderChunk, skip_empty_chunks: bool) -> bool {
    if !skip_empty_chunks {
        return true;
    }

    // Ignore chunks that are either empty or fully occluded
    if chunk.state.load() == ChunkState::ReadyEmpty {
        return false;
    }

    let Some(data) = chunk.data.as_ref() else {
        return false;
    };

    match data {
        ChunkData::Solid(v) => v.is_solid(),
        ChunkData::Packed(p) => p.palette.voxel_types.iter().any(Voxel::is_solid),
    }
}

fn pick_by_aabb_intersection(
    world: &RenderWorld,
    origin: Vec3,
    dir: Vec3,
    settings: &PickSettings,
) -> Option<ChunkPos> {
    let PickSettings {
        max_distance,
        skip_empty_chunks,
        use_mesh_aabb,
    } = *settings;

    let mut best_t = f32::INFINITY;
    let mut best_pos: Option<ChunkPos> = None;

    // This is a brute force search; could be optimized with an octree or similar structure
    for entry in world.chunks.iter() {
        if !is_pickable_render_chunk(entry.value(), skip_empty_chunks) {
            continue;
        }

        let pos = *entry.key();
        let (_label, aabb) = chunk_pick_aabb(pos, entry.value(), use_mesh_aabb);

        if let Some(t) = ray_aabb_tmin(origin, dir, &aabb)
            && t >= 0.0
            && t <= max_distance
            && t < best_t
        {
            best_t = t;
            best_pos = Some(pos);
        }
    }

    best_pos
}

fn ray_aabb_tmin(origin: Vec3, dir: Vec3, aabb: &AABB) -> Option<f32> {
    // https://en.wikipedia.org/wiki/Slab_method
    let mut tmin = -f32::INFINITY;
    let mut tmax = f32::INFINITY;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        let min = aabb.min[axis];
        let max = aabb.max[axis];

        if d.abs() < 1e-8 {
            // Ray parallel to slab: must be within bounds.
            if o < min || o > max {
                return None;
            }
            continue;
        }

        let inv_d = 1.0 / d;
        let mut t1 = (min - o) * inv_d;
        let mut t2 = (max - o) * inv_d;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }

        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmax < tmin {
            return None;
        }
    }

    Some(tmin)
}

fn pick_first_existing_chunk(
    world: &RenderWorld,
    origin: Vec3,
    dir: Vec3,
    settings: &PickSettings,
) -> Option<ChunkPos> {
    let PickSettings {
        max_distance,
        skip_empty_chunks,
        use_mesh_aabb,
    } = *settings;

    // 3D DDA through chunk grid.
    let cell_size = CHUNK_SIZE as f32;

    let mut cell = (origin / cell_size).floor().as_ivec3();

    // Step direction per axis (in chunk coordinates): -1, 0, +1.
    let step: IVec3 = dir.signum().as_ivec3();
    let step_nonzero = step.cmpne(IVec3::ZERO);

    // Next boundary in world-space for each axis.
    // For positive step, boundary is (cell + 1) * cell_size; otherwise it's cell * cell_size.
    let step_positive = step.cmpgt(IVec3::ZERO);
    let next_boundary_world =
        (cell.as_vec3() + Vec3::select(step_positive, Vec3::ONE, Vec3::ZERO)) * cell_size;

    let inf = Vec3::splat(f32::INFINITY);
    let mut t_max = Vec3::select(step_nonzero, (next_boundary_world - origin) / dir, inf);
    let t_delta = Vec3::select(step_nonzero, Vec3::splat(cell_size) / dir.abs(), inf);

    t_max = t_max.max(Vec3::ZERO);

    let max_steps = ((max_distance / cell_size).ceil() as usize).clamp(1, 10_000);
    for _ in 0..max_steps {
        // Advance along the axis with the closest boundary
        let mut axis = 0usize;
        let mut traveled = t_max[0];
        if t_max[1] < traveled {
            axis = 1;
            traveled = t_max[1];
        }
        if t_max[2] < traveled {
            axis = 2;
            traveled = t_max[2];
        }

        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];

        if traveled > max_distance {
            break;
        }

        let pos = ChunkPos(cell);
        let Some(chunk) = world.chunks.get(&pos) else {
            continue;
        };

        if !is_pickable_render_chunk(chunk.value(), skip_empty_chunks) {
            continue;
        }

        let (_label, aabb) = chunk_pick_aabb(pos, chunk.value(), use_mesh_aabb);
        if let Some(t) = ray_aabb_tmin(origin, dir, &aabb)
            && t >= 0.0
            && t <= max_distance
        {
            return Some(pos);
        }
    }

    None
}
