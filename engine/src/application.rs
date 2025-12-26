use std::sync::Arc;

use debounce::EventDebouncer;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

use crate::{
    config::{
        EngineConfig, create_window_attributes, get_engine_config, update_engine_config_file,
    },
    game_window::GameWindow,
    voxels::{
        chunk::{Chunk, PackedChunk},
        coord::{ChunkPos, LocalPos},
        voxel::Voxel,
        world::World,
    },
    worldgen::generate_noise_world,
};

pub struct Application {
    pub game_window: Option<GameWindow>,
    engine_config: EngineConfig,
    config_debouncer: EventDebouncer<EngineConfig>,
    world: World,
}

impl Application {
    pub fn new() -> Self {
        let engine_config = get_engine_config().unwrap_or_default();
        let config_debouncer = EventDebouncer::new(
            std::time::Duration::from_millis(200),
            move |config: EngineConfig| {
                update_engine_config_file(&config).unwrap();
            },
        );

        const HELLO: [[u8; 16]; 5] = [
            [1, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1],
            [1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1],
            [1, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1],
            [1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1],
            [1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1],
        ];

        let world = generate_noise_world();
        {
            let mut chunk = Chunk::Packed(PackedChunk::new());

            for (y, row) in HELLO.iter().enumerate() {
                for (x, &value) in row.iter().enumerate() {
                    if value != 0 {
                        chunk.set_voxel(LocalPos::new(x as u8, 5 - y as u8, 0), Voxel::STONE);
                    }
                }
            }

            world.chunks.insert(ChunkPos::new(0, 1, 0), chunk);
        }

        Application {
            game_window: None,
            engine_config,
            config_debouncer,
            world,
        }
    }
}

impl ApplicationHandler<GameWindow> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = create_window_attributes(&self.engine_config);
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.game_window = Some(pollster::block_on(GameWindow::new(window)).unwrap());
        self.game_window
            .as_mut()
            .unwrap()
            .world_renderer
            .create_all_chunks(&self.world);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: GameWindow) {
        self.game_window = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let game_window = match &mut self.game_window {
            Some(window) => window,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.engine_config.window_size = Some((size.width, size.height));
                self.config_debouncer.put(self.engine_config.clone());
                game_window.resize(size.width, size.height);
            }
            WindowEvent::Moved(position) => {
                if game_window.is_minimized() {
                    return;
                }
                self.engine_config.window_position = Some((position.x, position.y));
                self.config_debouncer.put(self.engine_config.clone());
            }
            WindowEvent::RedrawRequested => {
                game_window.render().unwrap();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => match (code, key_state) {
                (KeyCode::Escape, ElementState::Pressed) => {
                    event_loop.exit();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

pub fn run_application() -> anyhow::Result<()> {
    pretty_env_logger::init_timed();

    let mut app = Application::new();
    let event_loop: EventLoop<GameWindow> = EventLoop::with_user_event().build()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
