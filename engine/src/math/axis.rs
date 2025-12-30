use glam::{IVec3, U8Vec3};

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

    pub const fn u_axis(self) -> Axis {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::Z,
            Axis::Z => Axis::X,
        }
    }

    pub const fn v_axis(self) -> Axis {
        match self {
            Axis::X => Axis::Z,
            Axis::Y => Axis::X,
            Axis::Z => Axis::Y,
        }
    }
}

pub trait Vector3AxisExt<T> {
    fn get_axis(&self, axis: Axis) -> T;
    fn set_axis(&mut self, axis: Axis, value: T);
    fn from_axis_values(axises: [(Axis, T); 3]) -> Self;
    /// Returns a new vector with its components ordered according to the given axes.
    fn shuffle(&self, u_axis: Axis, v_axis: Axis, d_axis: Axis) -> Self;
}

impl Vector3AxisExt<i32> for IVec3 {
    fn get_axis(&self, axis: Axis) -> i32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: i32) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
    }

    fn from_axis_values(axises: [(Axis, i32); 3]) -> Self {
        let mut vec = IVec3::ZERO;
        for (axis, value) in axises.iter() {
            vec.set_axis(*axis, *value);
        }
        vec
    }

    fn shuffle(&self, u_axis: Axis, v_axis: Axis, d_axis: Axis) -> Self {
        IVec3::new(
            self.get_axis(u_axis),
            self.get_axis(v_axis),
            self.get_axis(d_axis),
        )
    }
}

impl Vector3AxisExt<u8> for U8Vec3 {
    fn get_axis(&self, axis: Axis) -> u8 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: u8) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
    }

    fn from_axis_values(axises: [(Axis, u8); 3]) -> Self {
        let mut vec = U8Vec3::ZERO;
        for (axis, value) in axises.iter() {
            vec.set_axis(*axis, *value);
        }
        vec
    }

    fn shuffle(&self, u_axis: Axis, v_axis: Axis, d_axis: Axis) -> Self {
        U8Vec3::new(
            self.get_axis(u_axis),
            self.get_axis(v_axis),
            self.get_axis(d_axis),
        )
    }
}
