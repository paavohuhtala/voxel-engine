use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4Swizzles, vec4};

use crate::math::aabb::AABB;
use crate::math::plane::Plane;

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Frustum {
    // Planes are in the order: left, right, bottom, top, near, far
    pub planes: [Plane; 6],
}

impl Frustum {
    fn corners(inverse_view_projection: &Mat4) -> [Vec3; 8] {
        let corners: [glam::Vec4; 8] = [
            // Left - Bottom - Near
            vec4(-1.0, -1.0, 1.0, 1.0),
            // Right - Bottom - Near
            vec4(1.0, -1.0, 1.0, 1.0),
            // Left - Top - Near
            vec4(-1.0, 1.0, 1.0, 1.0),
            // Right - Top - Near
            vec4(1.0, 1.0, 1.0, 1.0),
            // Left - Bottom - Far
            vec4(-1.0, -1.0, 0.00001, 1.0),
            // Right - Bottom - Far
            vec4(1.0, -1.0, 0.00001, 1.0),
            // Left - Top - Far
            vec4(-1.0, 1.0, 0.00001, 1.0),
            // Right - Top - Far
            vec4(1.0, 1.0, 0.00001, 1.0),
        ];

        corners.map(|corner| {
            let mut corner = inverse_view_projection * corner;
            corner = corner / corner.w;
            corner.xyz()
        })
    }

    pub fn from_inverse_view_projection(inverse_view_projection: &Mat4) -> Frustum {
        let corners = Self::corners(inverse_view_projection);
        let [
            left_bottom_near,
            right_bottom_near,
            left_top_near,
            right_top_near,
            left_bottom_far,
            right_bottom_far,
            left_top_far,
            _right_top_far,
        ] = corners;

        let planes = [
            // Left
            Plane::from_points(left_bottom_near, left_top_far, left_bottom_far),
            // Right
            Plane::from_points(right_bottom_near, right_bottom_far, right_top_near),
            // Bottom
            Plane::from_points(left_bottom_near, right_bottom_near, left_bottom_far).flip(),
            // Top
            Plane::from_points(left_top_near, right_top_near, left_top_far),
            // Near
            Plane::from_points(left_bottom_near, right_bottom_near, left_top_near),
            // Far
            Plane::from_points(left_bottom_far, right_bottom_far, left_top_far).flip(),
        ];

        Frustum { planes }
    }

    pub fn intersects_aabb(&self, aabb: &AABB) -> bool {
        let center = aabb.center();
        let extent = aabb.extent();

        for plane in &self.planes {
            let r = extent.dot(plane.normal.abs());
            let d = plane.distance_to_point(center);

            if d < -r {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};

    #[test]
    fn test_frustum_intersection() {
        let view = Mat4::look_at_lh(
            Vec3::new(0.0, 0.0, -5.0), // Eye
            Vec3::new(0.0, 0.0, 0.0),  // Center
            Vec3::new(0.0, 1.0, 0.0),  // Up
        );
        let projection = Mat4::perspective_infinite_reverse_lh(90.0_f32.to_radians(), 1.0, 0.1);
        let view_projection = projection * view;
        let frustum = Frustum::from_inverse_view_projection(&view_projection);

        // AABB at origin (should be visible)
        let aabb_inside = AABB::new(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        assert!(
            frustum.intersects_aabb(&aabb_inside),
            "Origin AABB should be visible"
        );

        // AABB behind camera (should be culled)
        // Camera is at (0,0,-5) looking at (0,0,0). Back is -Z.
        // Behind camera is < -5.
        let aabb_behind = AABB::new(Vec3::new(-0.5, -0.5, -7.0), Vec3::new(0.5, 0.5, -6.0));
        assert!(
            !frustum.intersects_aabb(&aabb_behind),
            "Behind camera AABB should be culled"
        );

        // AABB far to the right (should be culled)
        // FOV 90, aspect 1. At Z=0 (dist 5), width is approx 10.
        // Right plane is at X ~ Z.
        let aabb_right = AABB::new(Vec3::new(10.0, -0.5, -0.5), Vec3::new(11.0, 0.5, 0.5));
        assert!(
            !frustum.intersects_aabb(&aabb_right),
            "Right AABB should be culled"
        );
    }
}
