use glam::Vec3;
use splines::{Interpolation, Key, Spline};

use crate::{camera::Camera, game_loop::GameLoopTime};

#[allow(unused)]
enum CameraMode {
    Rotate { angle: f32 },
    Path { progress: f32 },
}

pub struct Player {
    pub camera: Camera,
    pub is_local: bool,

    camera_path: Spline<f32, Vec3>,
    camera_mode: CameraMode,
    pub should_move_camera: bool,
}

impl Player {
    pub fn new() -> Self {
        let camera = Camera::new(
            Vec3::new(64.0, 32.0, -32.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
        );

        let camera_path = Spline::from_vec(vec![
            (Key::new(0.0, Vec3::new(0.0, 32.0, 0.0), Interpolation::Linear)),
            (Key::new(
                50.0,
                Vec3::new(200.0, 35.0, 8000.0),
                Interpolation::CatmullRom,
            )),
            (Key::new(
                90.0,
                Vec3::new(4000.0, 40.0, 900.0),
                Interpolation::CatmullRom,
            )),
            (Key::new(100.0, Vec3::new(2500.0, 32.0, -50.0), Interpolation::Linear)),
        ]);

        Player {
            camera,
            is_local: true,
            camera_path,
            // camera_mode: CameraMode::Rotate { angle: 0.0 },
            camera_mode: CameraMode::Path { progress: 0.0 },
            should_move_camera: false,
        }
    }

    pub fn update(&mut self, time: &GameLoopTime) {
        if !self.should_move_camera {
            return;
        }

        let (eye, target) = match self.camera_mode {
            CameraMode::Path { mut progress } => {
                progress += time.delta_time_s as f32;

                if progress >= 89.0 {
                    progress = 0.0;
                }

                self.camera_mode = CameraMode::Path { progress };

                let eye =
                    self.camera_path.clamped_sample(progress).unwrap() + Vec3::new(0.0, 100.0, 0.0);
                let target = self.camera_path.clamped_sample(progress + 1.0).unwrap();

                (eye, target)
            }
            CameraMode::Rotate { mut angle } => {
                angle += time.delta_time_s as f32 * 0.5;
                self.camera_mode = CameraMode::Rotate { angle };

                let radius = 32.0;
                self.camera.eye = Vec3::new(angle.sin() * radius, 32.0, angle.cos() * radius);

                let target = Vec3::new(0.0, 0.0, 0.0);
                (self.camera.eye, target)
            }
        };

        self.camera.eye = eye;
        self.camera.target = target;
    }

    pub fn before_render(&mut self, resolution: glam::Vec2) {
        self.camera.update_matrices(resolution);
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
