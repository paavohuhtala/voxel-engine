use std::sync::{Arc, RwLock};

use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

use engine::{
    EngineContext,
    config::config_manager::ConfigManager,
    game_loop::{GameLoop, GameLoopConfig, GameLoopResult},
};

use crate::{client_game::ClientGame, config::ClientConfig};

pub struct Application {
    config: Arc<RwLock<ClientConfig>>,
    game_loop: GameLoop<ClientGame>,
}

impl Application {
    pub fn new(engine_context: EngineContext, client_config: ConfigManager<ClientConfig>) -> Self {
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
        }
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = self.config.read().unwrap().create_window_attributes();
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.game_loop.game.on_resumed(window);
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
