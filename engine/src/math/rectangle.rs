use glam::{IVec2, UVec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IRectangle {
    pub origin: IVec2,
    pub size: IVec2,
}

impl IRectangle {
    pub fn new(origin: IVec2, size: IVec2) -> Self {
        IRectangle { origin, size }
    }

    pub fn from_corners(min: IVec2, max: IVec2) -> Self {
        IRectangle {
            origin: min,
            size: max - min,
        }
    }
}

pub struct URectangle {
    pub origin: UVec2,
    pub size: UVec2,
}

impl URectangle {
    pub fn new(origin: UVec2, size: UVec2) -> Self {
        URectangle { origin, size }
    }

    pub fn from_corners(min: UVec2, max: UVec2) -> Self {
        URectangle {
            origin: min,
            size: max - min,
        }
    }
}
