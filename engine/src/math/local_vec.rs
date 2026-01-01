use std::ops::Add;

use glam::{IVec3, U8Vec3};

use crate::math::{
    abstract_vec::{AbstractVec2, AbstractVec3},
    axis::Axis,
    basis::{Basis, Basis2D},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVec3<TVec>
where
    TVec: AbstractVec3,
{
    pub vec: TVec,
    pub basis: Basis,
}

impl<TVec> LocalVec3<TVec>
where
    TVec: AbstractVec3,
{
    pub fn new(vec: TVec, basis: Basis) -> Self {
        Self { vec, basis }
    }

    pub fn to_world(&self) -> TVec {
        self.vec.local_to_world(self.basis)
    }

    pub fn from_world(vec: TVec, basis: Basis) -> Self {
        Self {
            vec: vec.world_to_local(basis),
            basis,
        }
    }

    pub fn u(&self) -> TVec::Element {
        self.vec.get_axis(Axis::X)
    }

    pub fn v(&self) -> TVec::Element {
        self.vec.get_axis(Axis::Y)
    }

    pub fn d(&self) -> TVec::Element {
        self.vec.get_axis(Axis::Z)
    }

    pub fn d_mut(&mut self) -> &mut TVec::Element {
        self.vec.get_axis_mut(Axis::Z)
    }

    pub fn offset(
        &self,
        u_offset: TVec::Element,
        v_offset: TVec::Element,
        d_offset: TVec::Element,
    ) -> Self {
        let mut new_vec = self.vec;
        *new_vec.get_axis_mut(Axis::X) += u_offset;
        *new_vec.get_axis_mut(Axis::Y) += v_offset;
        *new_vec.get_axis_mut(Axis::Z) += d_offset;
        Self {
            vec: new_vec,
            basis: self.basis,
        }
    }

    pub fn uv(&self) -> LocalVec2<TVec::Vector2D> {
        let local_vec = TVec::Vector2D::new(self.u(), self.v());
        LocalVec2 {
            vec: local_vec,
            basis: Basis2D::new(self.basis.u, self.basis.v),
        }
    }
}

pub trait ConstructLocalVec3<T> {
    type Vector: AbstractVec3<Element = T>;

    fn from_uvd(u: T, v: T, d: T, basis: Basis) -> LocalVec3<Self::Vector>
    where
        Self: Sized,
        T: Copy + Sized,
    {
        let vec = Self::Vector::new(u, v, d);
        LocalVec3::new(vec, basis)
    }
}

impl ConstructLocalVec3<i32> for LocalVec3<IVec3> {
    type Vector = IVec3;
}

impl ConstructLocalVec3<u8> for LocalVec3<U8Vec3> {
    type Vector = U8Vec3;
}

impl<TVec: AbstractVec3> Add for LocalVec3<TVec> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        assert_eq!(
            self.basis, rhs.basis,
            "Cannot add LocalVec3 with different basis"
        );
        let vec = self.vec + rhs.vec;
        Self {
            vec,
            basis: self.basis,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVec2<TVec>
where
    TVec: AbstractVec2,
{
    pub vec: TVec,
    pub basis: Basis2D,
}

impl<TVec> LocalVec2<TVec>
where
    TVec: AbstractVec2,
{
    pub fn new(vec: TVec, basis: Basis2D) -> Self {
        Self { vec, basis }
    }

    pub fn u(&self) -> TVec::Element {
        self.vec.get_axis(Axis::X)
    }

    pub fn v(&self) -> TVec::Element {
        self.vec.get_axis(Axis::Y)
    }

    pub fn offset(&self, u_offset: TVec::Element, v_offset: TVec::Element) -> Self {
        let mut new_vec = self.vec;
        *new_vec.get_axis_mut(Axis::X) += u_offset;
        *new_vec.get_axis_mut(Axis::Y) += v_offset;
        Self {
            vec: new_vec,
            basis: self.basis,
        }
    }

    pub fn extend(&self, d: TVec::Element, d_axis: Axis) -> LocalVec3<TVec::Vector3D> {
        let basis = self.basis.extend(d_axis);
        let vec = TVec::Vector3D::new(self.vec.get_axis(Axis::X), self.vec.get_axis(Axis::Y), d);
        LocalVec3 { vec, basis }
    }
}
