use glam::Vec3;
use splines::{Interpolation, Key, Spline};

use crate::{camera::Camera, game_loop::GameLoopTime};

pub struct Player {
    pub camera: Camera,
    pub is_local: bool,

    path_progress: f32,
    camera_path: Spline<f32, Vec3>,
}

impl Player {
    pub fn new() -> Self {
        let camera = Camera {
            eye: Vec3::new(64.0, 32.0, -32.0),
            target: Vec3::new(0.0, 0.0, 0.0),
            up: Vec3::Y,
        };

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
            path_progress: 0.0,
            camera_path,
        }
    }

    pub fn update(&mut self, time: &GameLoopTime) {
        self.path_progress += time.delta_time_s as f32;

        if self.path_progress >= 89.0 {
            self.path_progress = 0.0;
        }

        let eye = self.camera_path.clamped_sample(self.path_progress).unwrap();
        let target = self
            .camera_path
            .clamped_sample(self.path_progress + 1.0)
            .unwrap();

        self.camera = Camera {
            eye,
            target,
            up: Vec3::Y,
        };
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
