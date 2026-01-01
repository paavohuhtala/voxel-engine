use std::ops::{Add, AddAssign};

use glam::{IVec2, IVec3, U8Vec2, U8Vec3};

use crate::math::{axis::Axis, basis::Basis};

// This is basically a poor subset of nalgebra, built around glam and our own basis/axis system

pub trait AbstractVec3: Copy + Sized + Add<Output = Self> + Default {
    type Element: Copy + Sized + Default + Add<Output = Self::Element> + AddAssign;
    type Vector2D: AbstractVec2<Element = Self::Element>;

    fn new(x: Self::Element, y: Self::Element, z: Self::Element) -> Self;
    fn get_axis(&self, axis: Axis) -> Self::Element;
    fn get_axis_mut(&mut self, axis: Axis) -> &mut Self::Element;
    fn set_axis(&mut self, axis: Axis, value: Self::Element);

    fn from_axis_values(axises: [(Axis, Self::Element); 3]) -> Self {
        let mut vec = Self::default();
        for (axis, value) in axises.iter() {
            vec.set_axis(*axis, *value);
        }
        vec
    }

    fn world_to_local(&self, local: Basis) -> Self {
        Self::new(
            self.get_axis(local.u),
            self.get_axis(local.v),
            self.get_axis(local.d),
        )
    }

    fn local_to_world(&self, local: Basis) -> Self {
        let mut vec = Self::default();
        vec.set_axis(local.u, self.get_axis(Axis::X));
        vec.set_axis(local.v, self.get_axis(Axis::Y));
        vec.set_axis(local.d, self.get_axis(Axis::Z));
        vec
    }
}

impl AbstractVec3 for IVec3 {
    type Element = i32;
    type Vector2D = IVec2;

    fn new(x: i32, y: i32, z: i32) -> Self {
        IVec3::new(x, y, z)
    }

    fn get_axis(&self, axis: Axis) -> i32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    fn get_axis_mut(&mut self, axis: Axis) -> &mut i32 {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: i32) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
    }
}

impl AbstractVec3 for U8Vec3 {
    type Element = u8;
    type Vector2D = U8Vec2;

    fn new(x: u8, y: u8, z: u8) -> Self {
        U8Vec3::new(x, y, z)
    }

    fn get_axis(&self, axis: Axis) -> u8 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    fn get_axis_mut(&mut self, axis: Axis) -> &mut u8 {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: u8) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
    }
}

pub trait AbstractVec2: Copy + Sized + Add<Output = Self> + Default {
    type Element: Copy + Sized + Default + Add<Output = Self::Element> + AddAssign;
    type Vector3D: AbstractVec3<Element = Self::Element>;

    fn new(x: Self::Element, y: Self::Element) -> Self;
    fn get_axis(&self, axis: Axis) -> Self::Element;
    fn get_axis_mut(&mut self, axis: Axis) -> &mut Self::Element;
    fn set_axis(&mut self, axis: Axis, value: Self::Element);
}

impl AbstractVec2 for U8Vec2 {
    type Element = u8;
    type Vector3D = U8Vec3;

    fn new(x: u8, y: u8) -> Self {
        U8Vec2::new(x, y)
    }

    fn get_axis(&self, axis: Axis) -> u8 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            _ => panic!("U8Vec2 only supports X and Y axes"),
        }
    }

    fn get_axis_mut(&mut self, axis: Axis) -> &mut u8 {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            _ => panic!("U8Vec2 only supports X and Y axes"),
        }
    }

    fn set_axis(&mut self, axis: Axis, value: u8) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            _ => panic!("U8Vec2 only supports X and Y axes"),
        }
    }
}

impl AbstractVec2 for IVec2 {
    type Element = i32;
    type Vector3D = IVec3;

    fn new(x: i32, y: i32) -> Self {
        IVec2::new(x, y)
    }

    fn get_axis(&self, axis: Axis) -> i32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            _ => panic!("IVec2 only supports X and Y axes"),
        }
    }

    fn get_axis_mut(&mut self, axis: Axis) -> &mut i32 {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            _ => panic!("IVec2 only supports X and Y axes"),
        }
    }

    fn set_axis(&mut self, axis: Axis, value: i32) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            _ => panic!("IVec2 only supports X and Y axes"),
        }
    }
}
