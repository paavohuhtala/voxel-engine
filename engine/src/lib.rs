use std::{path::PathBuf, sync::Arc};

use crate::{
    assets::{blocks::BlockDatabase, fonts::load_font},
    config::{
        config_manager::{Config, ConfigManager},
        engine_config::EngineConfig,
    },
    gameplay::physics::world_collider::PhysicsWorld,
    voxels::{coord::WorldPos, world::World},
    worldgen::{generate_noise_world, text_generator::draw_text},
};

pub mod assets;
pub mod camera;
pub mod config;
pub mod game_loop;
pub mod gameplay;
pub mod math;
pub mod memory;
pub mod voxels;
pub mod worldgen;

pub struct EngineContext {
    pub config: ConfigManager<EngineConfig>,
    pub world: World,
    pub block_database: Arc<BlockDatabase>,
    pub physics: PhysicsWorld,
}

pub fn init_engine() -> anyhow::Result<EngineContext> {
    let config = EngineConfig::create_manager()?;

    let font =
        load_font(PathBuf::from("assets/fonts").as_path(), "custom").expect("Failed to load font");

    let mut block_database = BlockDatabase::new();
    block_database
        .load_all_blocks()
        .expect("Failed to load block definitions");
    let block_database = Arc::new(block_database);

    let world = generate_noise_world(16);
    draw_text(&world, WorldPos::new(0, 16, 0), &font, "Hello, world!");

    let mut physics = PhysicsWorld::new();
    physics.add_all_chunks(&world);
    physics.spawn_debug_ball();

    Ok(EngineContext {
        config,
        world,
        block_database,
        physics,
    })
}
