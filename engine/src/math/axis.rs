use glam::IVec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X = 0,
    Y,
    Z,
}

impl Axis {
    pub const fn as_unit_vector(self) -> IVec3 {
        match self {
            Axis::X => IVec3::new(1, 0, 0),
            Axis::Y => IVec3::new(0, 1, 0),
            Axis::Z => IVec3::new(0, 0, 1),
        }
    }

    /// Returns the axis corresponding to the U coordinate (Tangent) on a face perpendicular to this axis.
    /// These are chosen so that tangent × bitangent points in the positive direction of the depth axis.
    pub const fn u_axis(self) -> Axis {
        match self {
            Axis::X => Axis::Z,
            Axis::Y => Axis::Z,
            Axis::Z => Axis::X,
        }
    }

    /// Returns the axis corresponding to the V coordinate (Bitangent) on a face perpendicular to this axis.
    /// These are chosen so that tangent × bitangent points in the positive direction of the depth axis.
    pub const fn v_axis(self) -> Axis {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
            Axis::Z => Axis::Y,
        }
    }
}
