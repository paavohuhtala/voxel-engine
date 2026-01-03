use std::collections::HashMap;

use glam::Vec3;
use parry3d::math::Point;
use rapier3d::prelude::{
    CCDSolver, Collider, ColliderBuilder, ColliderHandle, ColliderSet, DefaultBroadPhase,
    ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet, NarrowPhase,
    PhysicsPipeline, RigidBodyBuilder, RigidBodyHandle, RigidBodySet,
};

use crate::voxels::{chunk::Chunk, coord::ChunkPos};

pub struct PhysicsWorld {
    colliders: ColliderSet,
    rigid_bodies: RigidBodySet,
    chunks: HashMap<ChunkPos, ChunkHandles>,

    gravity: Vec3,

    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    physics_hooks: (),
    event_handler: (),
}

pub struct ChunkHandles {
    pub collider_handle: ColliderHandle,
    pub rigid_body_handle: RigidBodyHandle,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        PhysicsWorld {
            colliders: ColliderSet::new(),
            rigid_bodies: RigidBodySet::new(),
            chunks: HashMap::new(),
            gravity: Vec3::new(0.0, -9.81, 0.0),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            physics_hooks: (),
            event_handler: (),
        }
    }

    pub fn spawn_debug_ball(&mut self) {
        let rigid_body = RigidBodyBuilder::dynamic()
            .translation(Vec3::new(0.0, 32.0, 0.0).into())
            .build();
        let collider = ColliderBuilder::ball(0.5).restitution(0.7).build();

        let rigid_body_handle = self.rigid_bodies.insert(rigid_body);
        self.colliders
            .insert_with_parent(collider, rigid_body_handle, &mut self.rigid_bodies);
    }

    pub fn add_chunk(&mut self, chunk_pos: ChunkPos, chunk: &Chunk) {
        let collider = create_chunk_collider(chunk_pos, chunk);
        let rigid_body = RigidBodyBuilder::fixed().build();
        let rigid_body_handle = self.rigid_bodies.insert(rigid_body);
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, rigid_body_handle, &mut self.rigid_bodies);
        let handles = ChunkHandles {
            collider_handle,
            rigid_body_handle,
        };
        self.chunks.insert(chunk_pos, handles);
    }

    pub fn update(&mut self, delta_time: f32) {
        self.integration_parameters.dt = delta_time;

        self.physics_pipeline.step(
            &self.gravity.into(),
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_bodies,
            &mut self.colliders,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            &self.physics_hooks,
            &self.event_handler,
        );
    }
}

fn create_chunk_collider(chunk_pos: ChunkPos, chunk: &Chunk) -> Collider {
    let points = chunk
        .iter_voxels()
        .filter_map(|(local_pos, voxel)| -> Option<Point<i32>> {
            if voxel.is_solid() {
                Some(local_pos.0.as_ivec3().into())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    ColliderBuilder::voxels(Vec3::splat(1.0).into(), &points)
        .translation(chunk_pos.origin().0.as_vec3().into())
        .build()
}
