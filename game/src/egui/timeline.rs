use engine::player::Player;

pub fn draw_timeline(player: &mut Player, context: &egui::Context) {
    egui::Window::new("Timeline")
        .default_pos((260.0, 0.0))
        .title_bar(false)
        .min_width(900.0)
        .show(context, |ui| {
            ui.label("Camera Path Progress");
            let progress = &mut player.camera_progress;
            ui.add(
                egui::Slider::new(progress, 0.0..=1.0)
                    .text("Progress")
                    .show_value(true),
            );
        });
}
