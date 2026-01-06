use engine::{
    config::config_manager::Config,
    init_engine,
    worldgen::{generate_noise_world, generate_torture_test_world},
};
use winit::event_loop::{ControlFlow, EventLoop};

use crate::{application::Application, config::ClientConfig};

mod application;
mod client_game;
mod client_types;
mod config;
mod egui;
mod fps_counter;

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init_timed();
    log::info!("Starting game client...");

    //let world = generate_torture_test_world();
    let context = init_engine()?;
    let client_config = ClientConfig::create_manager()?;
    let mut app = Application::new(
        context,
        client_config,
        Box::new(|block_database, render_context| {
            generate_noise_world(4, block_database, render_context)
        }),
    );

    let event_loop: EventLoop<()> = EventLoop::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app)?;

    Ok(())
}
