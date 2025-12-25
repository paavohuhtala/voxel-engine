use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    pub fn new(point1: Vec3, point2: Vec3) -> AABB {
        let min = point1.min(point2);
        let max = point1.max(point2);
        AABB { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn extent(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
}
