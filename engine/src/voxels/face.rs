use glam::{IVec3, U8Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Face {
    /// Y+
    Top = 0,
    /// Y-
    Bottom,
    /// X-
    Left,
    /// X+
    Right,
    /// Z-
    Front,
    /// Z+
    Back,
}

impl Default for Face {
    fn default() -> Self {
        Face::Top
    }
}

impl Face {
    pub fn to_ivec3(&self) -> IVec3 {
        match self {
            Face::Top => IVec3::Y,
            Face::Bottom => -IVec3::Y,
            Face::Left => -IVec3::X,
            Face::Right => IVec3::X,
            Face::Front => -IVec3::Z,
            Face::Back => IVec3::Z,
        }
    }

    pub fn all() -> [Face; 6] {
        [
            Face::Top,
            Face::Bottom,
            Face::Left,
            Face::Right,
            Face::Front,
            Face::Back,
        ]
    }

    pub fn vertices(self) -> [U8Vec3; 4] {
        match self {
            Face::Top => [
                U8Vec3::new(0, 1, 0),
                U8Vec3::new(1, 1, 0),
                U8Vec3::new(1, 1, 1),
                U8Vec3::new(0, 1, 1),
            ],
            Face::Bottom => [
                U8Vec3::new(0, 0, 0),
                U8Vec3::new(0, 0, 1),
                U8Vec3::new(1, 0, 1),
                U8Vec3::new(1, 0, 0),
            ],
            Face::Left => [
                U8Vec3::new(0, 0, 0),
                U8Vec3::new(0, 1, 0),
                U8Vec3::new(0, 1, 1),
                U8Vec3::new(0, 0, 1),
            ],
            Face::Right => [
                U8Vec3::new(1, 0, 0),
                U8Vec3::new(1, 0, 1),
                U8Vec3::new(1, 1, 1),
                U8Vec3::new(1, 1, 0),
            ],
            Face::Front => [
                U8Vec3::new(0, 0, 0),
                U8Vec3::new(0, 1, 0),
                U8Vec3::new(1, 1, 0),
                U8Vec3::new(1, 0, 0),
            ],
            Face::Back => [
                U8Vec3::new(0, 0, 1),
                U8Vec3::new(1, 0, 1),
                U8Vec3::new(1, 1, 1),
                U8Vec3::new(0, 1, 1),
            ],
        }
    }

    pub fn indices(self, start_index: u16) -> [u16; 6] {
        [
            start_index,
            start_index + 1,
            start_index + 2,
            start_index,
            start_index + 2,
            start_index + 3,
        ]
    }
}
