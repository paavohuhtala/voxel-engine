use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use egui::CornerRadius;

const MEASUREMENTS: usize = 120;

pub struct FpsCounter {
    frame_durations: VecDeque<Duration>,
    last_frame_time: Instant,
}

impl FpsCounter {
    pub fn new() -> Self {
        FpsCounter {
            frame_durations: VecDeque::with_capacity(MEASUREMENTS),
            last_frame_time: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame_time);
        self.last_frame_time = now;

        if self.frame_durations.len() == MEASUREMENTS {
            self.frame_durations.pop_front();
        }
        self.frame_durations.push_back(delta);
    }

    pub fn average_frame_time(&self) -> Duration {
        if self.frame_durations.is_empty() {
            // Default to 60 FPS-equivalent frame time, to avoid division by zero
            return Duration::from_millis(16);
        }

        let sum: Duration = self.frame_durations.iter().sum();
        sum / (self.frame_durations.len() as u32)
    }

    pub fn draw_ui(&self, context: &egui::Context) {
        let avg_time = self.average_frame_time();
        let ms = avg_time.as_secs_f64() * 1000.0;
        let fps = 1.0 / avg_time.as_secs_f32();

        egui::Window::new("Performance")
            .default_pos((0.0, 0.0))
            .default_width(250.0)
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .show(context, |ui| {
                ui.style_mut().visuals.window_corner_radius = CornerRadius::ZERO;

                egui::Grid::new("measurement_grid")
                    .num_columns(2)
                    .spacing((8.0, 4.0))
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Frame time:");
                        ui.label(format!("{:.2} ms", ms));
                        ui.end_row();

                        ui.label("FPS:");
                        ui.label(format!("{:.2}", fps));
                        ui.end_row();
                    });
            });
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new()
    }
}
