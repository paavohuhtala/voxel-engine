use bytemuck::{Pod, Zeroable};
use glam::{U8Vec3, Vec3};

#[derive(Debug, Clone, Copy, Default)]
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

#[derive(Debug, Clone, Copy, Default)]
pub struct AABB8 {
    pub min: U8Vec3,
    pub max: U8Vec3,
}

impl AABB8 {
    pub fn new(min: U8Vec3, max: U8Vec3) -> AABB8 {
        AABB8 { min, max }
    }

    pub fn center(&self) -> Vec3 {
        (self.min.as_vec3() + self.max.as_vec3()) * 0.5
    }

    pub fn extent(&self) -> Vec3 {
        (self.max.as_vec3() - self.min.as_vec3()) * 0.5
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Zeroable, Pod)]
// Since our chunk size is 16x16x16, we need just 4 bits per axis for min and max,
// resulting in a total of 24 bits. We can pack this into a u32 with 8 bits to spare.
pub struct PackedAABB(u32);

impl TryFrom<AABB8> for PackedAABB {
    type Error = &'static str;

    fn try_from(aabb: AABB8) -> Result<Self, Self::Error> {
        if !aabb.min.cmple(aabb.max).all() {
            return Err("Invalid AABB8: min must be less than max on all axes");
        }

        let max_values = U8Vec3::splat(16);

        if !(aabb.min.cmplt(max_values).all() && aabb.max.cmplt(max_values).all()) {
            return Err("Invalid AABB8: min and max must be in range [0, 16)");
        }

        let packed_min =
            (aabb.min.x as u32) | ((aabb.min.y as u32) << 4) | ((aabb.min.z as u32) << 8);

        let packed_max =
            (aabb.max.x as u32) | ((aabb.max.y as u32) << 4) | ((aabb.max.z as u32) << 8);

        Ok(PackedAABB(packed_min | (packed_max << 12)))
    }
}
