use bytesize::ByteSize;
use egui::CornerRadius;
use engine::voxels::chunk::ChunkState;
use renderer::{renderer_types::RenderWorld, rendering::world_renderer::WorldRenderer};

pub fn draw_world_stats_ui(
    world_renderer: &WorldRenderer,
    world: &RenderWorld,
    context: &egui::Context,
) {
    let renderer_stats = world_renderer.get_statistics();
    let world_stats = world.get_statistics();
    let report = &renderer_stats.face_buffer_storage_report;

    // Calculate percentages and format sizes
    let chunk_usage_pct = renderer_stats.chunk_buffer_used as f64
        / renderer_stats.chunk_buffer_capacity as f64
        * 100.0;

    let face_buffer_bytes = renderer_stats.face_buffer_capacity_bytes;
    let face_free_bytes = report.total_free_space as u64 * 4;
    let face_used_bytes = face_buffer_bytes - face_free_bytes;
    let face_usage_pct = face_used_bytes as f64 / face_buffer_bytes as f64 * 100.0;
    let largest_free_bytes = report.largest_free_region as u64 * 4;

    egui::Window::new("World statistics")
        .default_pos((0.0, 80.0))
        .default_width(300.0)
        .resizable(false)
        .movable(false)
        .show(context, |ui| {
            ui.style_mut().visuals.window_corner_radius = CornerRadius::ZERO;

            egui::Grid::new("world_stats_grid")
                .num_columns(2)
                .spacing((8.0, 4.0))
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Loaded chunks:");
                    ui.label(format!("{}", world_stats.total_loaded_chunks));
                    ui.end_row();

                    ui.label("World memory:");
                    ui.label(
                        ByteSize(world_stats.approximate_memory_usage_bytes as u64).to_string(),
                    );
                    ui.end_row();

                    ui.label("");
                    ui.label("");
                    ui.end_row();

                    ui.label("Chunk states:");
                    ui.end_row();
                    for state in ChunkState::all() {
                        let entry = world_stats
                            .chunks_by_state
                            .get(state)
                            .copied()
                            .unwrap_or_default();
                        ui.label(format!("{:?}:", state));
                        ui.label(format!("{}", entry));
                        ui.end_row();
                    }

                    ui.label("");
                    ui.label("");
                    ui.end_row();

                    ui.label("Chunk buffer:");
                    ui.label(format!(
                        "{} / {} ({:.1}%)",
                        renderer_stats.chunk_buffer_used,
                        renderer_stats.chunk_buffer_capacity,
                        chunk_usage_pct
                    ));
                    ui.end_row();

                    ui.label("Face buffer size:");
                    ui.label(ByteSize(face_buffer_bytes).to_string());
                    ui.end_row();

                    ui.label("Face buffer used:");
                    ui.label(format!(
                        "{} ({:.1}%)",
                        ByteSize(face_used_bytes),
                        face_usage_pct
                    ));
                    ui.end_row();

                    ui.label("Face buffer free:");
                    ui.label(ByteSize(face_free_bytes).to_string());
                    ui.end_row();

                    ui.label("Largest free region:");
                    ui.label(ByteSize(largest_free_bytes).to_string());
                    ui.end_row();
                });
        });
}
