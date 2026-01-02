use engine::camera::Camera;
use glam::{Quat, Vec3};

pub trait Mixable: Clone {
    fn mix(&self, other: &Self, factor: f64) -> Self;
}

impl Mixable for Vec3 {
    fn mix(&self, other: &Self, factor: f64) -> Self {
        self.lerp(*other, factor as f32)
    }
}

impl Mixable for Quat {
    fn mix(&self, other: &Self, factor: f64) -> Self {
        self.slerp(*other, factor as f32)
    }
}

impl Mixable for Camera {
    fn mix(&self, other: &Self, factor: f64) -> Self {
        Self {
            eye: self.eye.mix(&other.eye, factor),
            target: self.target.mix(&other.target, factor),
            up: self.up.mix(&other.up, factor),
        }
    }
}

pub struct Interpolated<T> {
    pub previous: T,
    pub current: T,
}

impl<T: Mixable> Interpolated<T> {
    pub fn new(initial: T) -> Self {
        Self {
            previous: initial.clone(),
            current: initial,
        }
    }

    pub fn get(&self, factor: f64) -> T {
        self.previous.mix(&self.current, factor)
    }

    pub fn set(&mut self, new: T) {
        self.previous = self.current.clone();
        self.current = new;
    }

    /// Sets both previous and current to the new value.
    /// Should be used for teleporting and other instant changes.
    pub fn set_immediate(&mut self, new: T) {
        self.previous = new.clone();
        self.current = new;
    }
}
