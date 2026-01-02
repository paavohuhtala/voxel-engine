use engine::{config::config_manager::Config, init_engine};
use winit::event_loop::EventLoop;

use crate::{application::Application, config::ClientConfig};

mod application;
mod client_game;
mod config;

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init_timed();
    log::info!("Starting game client...");

    let context = init_engine()?;
    let client_config = ClientConfig::create_manager()?;
    let mut app = Application::new(context, client_config);
    let event_loop: EventLoop<()> = EventLoop::with_user_event().build()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
