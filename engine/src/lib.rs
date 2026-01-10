use std::{path::PathBuf, sync::Arc};

use crate::{
    assets::{blocks::BlockDatabase, fonts::load_font},
    config::{
        config_manager::{Config, ConfigManager},
        engine_config::EngineConfig,
    },
    gameplay::physics::world_collider::PhysicsWorld,
    player::Player,
    voxels::chunk::IChunkRenderState,
    world::World,
};

pub mod assets;
pub mod camera;
pub mod chunk_loader;
pub mod config;
pub mod game_loop;
pub mod gameplay;
pub mod limits;
pub mod loader_job_queue;
pub mod math;
pub mod memory;
pub mod mesh_generation;
pub mod player;
pub mod visibility;
pub mod voxels;
pub mod world;
pub mod world_stats;
pub mod worldgen;

pub struct EngineContext<T: IChunkRenderState = ()> {
    pub config: ConfigManager<EngineConfig>,
    pub world: Option<World<T>>,
    pub block_database: Arc<BlockDatabase>,
    pub physics: PhysicsWorld,
    // TODO: Multiplayer, make this optional
    pub player: Player,
}

impl<T: IChunkRenderState> EngineContext<T> {
    pub fn set_world(&mut self, world: World<T>) {
        self.world = Some(world);
    }
}

// TODO: Engine should be able to init without a world
pub fn init_engine<T: IChunkRenderState>() -> anyhow::Result<EngineContext<T>> {
    let config = EngineConfig::create_manager()?;

    // TODO: This does nothing, this is just here to ensure the font loading system works
    load_font(PathBuf::from("assets/fonts").as_path(), "custom").expect("Failed to load font");

    let mut block_database = BlockDatabase::new();
    block_database
        .load_all_blocks()
        .expect("Failed to load block definitions");
    let block_database = Arc::new(block_database);
    let physics = PhysicsWorld::new();

    Ok(EngineContext {
        config,
        world: None,
        block_database,
        physics,
        player: Player::new(),
    })
}
