use glam::Vec3;

pub enum PlayerState {
    NoClip,
    Walking,
}

pub struct FirstPersonController {
    pub position: Vec3,
}
