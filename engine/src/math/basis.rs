use crate::math::axis::Axis;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Basis {
    pub u: Axis,
    pub v: Axis,
    pub d: Axis,
}

impl Basis {
    pub const fn new(u: Axis, v: Axis, d: Axis) -> Self {
        Self { u, v, d }
    }

    pub const fn uv(&self) -> Basis2D {
        Basis2D {
            u: self.u,
            v: self.v,
        }
    }
}

impl Default for Basis {
    fn default() -> Self {
        Self {
            u: Axis::X,
            v: Axis::Y,
            d: Axis::Z,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Basis2D {
    pub u: Axis,
    pub v: Axis,
}

impl Basis2D {
    pub const fn new(u: Axis, v: Axis) -> Self {
        Self { u, v }
    }

    pub const fn extend(&self, d: Axis) -> Basis {
        Basis {
            u: self.u,
            v: self.v,
            d,
        }
    }
}

impl Default for Basis2D {
    fn default() -> Self {
        Self {
            u: Axis::X,
            v: Axis::Y,
        }
    }
}
