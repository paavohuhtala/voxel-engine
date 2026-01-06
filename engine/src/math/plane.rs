use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[derive(Copy, Clone, Pod, Zeroable, Debug, Default)]
#[repr(C)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

impl Plane {
    pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Plane {
        let ab = b - a;
        let ac = c - a;
        let normal = ab.cross(ac).normalize();
        let distance = -normal.dot(a);
        Plane { normal, distance }
    }

    pub fn flip(&self) -> Plane {
        Plane {
            normal: -self.normal,
            distance: -self.distance,
        }
    }

    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}
