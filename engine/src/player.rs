use glam::Vec3;
use splines::{Interpolation, Key, Spline};

use crate::{camera::Camera, game_loop::GameLoopTime};

#[allow(unused)]
enum CameraMode {
    Rotate,
    Path,
}

pub struct Player {
    pub camera: Camera,
    pub is_local: bool,

    pub camera_progress: f32,
    pub camera_angle: f32,
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
                0.5,
                Vec3::new(200.0, 35.0, 8000.0),
                Interpolation::CatmullRom,
            )),
            (Key::new(
                0.9,
                Vec3::new(4000.0, 40.0, 900.0),
                Interpolation::CatmullRom,
            )),
            (Key::new(1.0, Vec3::new(2500.0, 32.0, -50.0), Interpolation::Linear)),
        ]);

        Player {
            camera,
            is_local: true,
            camera_path,
            camera_progress: 0.0,
            camera_angle: 0.0,
            // camera_mode: CameraMode::Rotate { angle: 0.0 },
            camera_mode: CameraMode::Path,
            should_move_camera: false,
        }
    }

    pub fn update(&mut self, time: &GameLoopTime) {
        if self.should_move_camera {
            match self.camera_mode {
                CameraMode::Path => {
                    self.camera_progress =
                        (self.camera_progress + time.delta_time_s as f32 * 0.01).min(1.0);
                }
                CameraMode::Rotate => {
                    self.camera_angle += time.delta_time_s as f32 * 0.5;
                }
            }
        }

        let (eye, target) = match self.camera_mode {
            CameraMode::Path => {
                self.camera_mode = CameraMode::Path;
                let eye = self
                    .camera_path
                    .clamped_sample(self.camera_progress)
                    .unwrap()
                    + Vec3::new(0.0, 100.0, 0.0);
                let target = self
                    .camera_path
                    .clamped_sample((self.camera_progress + 0.005).min(1.0))
                    .unwrap();

                (eye, target)
            }
            CameraMode::Rotate => {
                self.camera_mode = CameraMode::Rotate;

                let radius = 32.0;
                self.camera.eye = Vec3::new(
                    self.camera_angle.sin() * radius,
                    32.0,
                    self.camera_angle.cos() * radius,
                );
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
