use std::sync::{Arc, RwLock};

use renderer::{renderer_types::RenderWorld, rendering::world_renderer::ChunkRenderContext};
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

use engine::{
    assets::blocks::BlockDatabaseSlim,
    config::config_manager::ConfigManager,
    game_loop::{GameLoop, GameLoopConfig, GameLoopResult},
};

use crate::{client_game::ClientGame, client_types::ClientEngineContext, config::ClientConfig};

pub struct Application {
    config: Arc<RwLock<ClientConfig>>,
    game_loop: GameLoop<ClientGame>,
    world_creator: Option<RenderWorldCreator>,
}

pub type RenderWorldCreator =
    Box<dyn FnOnce(Arc<BlockDatabaseSlim>, ChunkRenderContext) -> RenderWorld>;

impl Application {
    pub fn new(
        engine_context: ClientEngineContext,
        client_config: ConfigManager<ClientConfig>,
        world_creator: RenderWorldCreator,
    ) -> Self {
        let config = client_config.get();
        let game = ClientGame::new(engine_context, client_config);
        Application {
            config,
            game_loop: GameLoop::new(
                game,
                GameLoopConfig {
                    updates_per_s: 60,
                    max_frame_time_s: 0.2,
                },
            ),
            world_creator: Some(world_creator),
        }
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = self.config.read().unwrap().create_window_attributes();
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.game_loop.game.on_resumed(window);
        let world_creator = self
            .world_creator
            .take()
            .expect("World creator function was already taken");

        let renderer = self
            .game_loop
            .game
            .renderer
            .as_ref()
            .expect("Renderer should exist after resume");

        let render_context = renderer.world_renderer.create_chunk_render_context();
        let block_database_slim =
            BlockDatabaseSlim::from_block_database(&self.game_loop.game.ctx.block_database);
        let block_database_slim = Arc::new(block_database_slim);

        let world = world_creator(block_database_slim, render_context);
        self.game_loop.game.ctx.set_world(world);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(egui_renderer) = &mut self.game_loop.game.egui {
            // TODO: If egui handles the event, don't pass it to the game
            let _ = egui_renderer.handle_input(&event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.game_loop.game.on_window_resized(size);
            }
            WindowEvent::Moved(position) => {
                self.game_loop.game.on_window_moved(position);
            }
            WindowEvent::RedrawRequested => {
                if self.game_loop.game.should_exit() {
                    event_loop.exit();
                    return;
                }

                match self.game_loop.next_frame().unwrap() {
                    GameLoopResult::Continue => {}
                    GameLoopResult::Exit => event_loop.exit(),
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.game_loop.game.on_key_event(&event);
            }
            _ => {}
        }
    }
}
