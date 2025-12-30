use std::{path::PathBuf, sync::Arc};

use debounce::EventDebouncer;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

use crate::{
    assets::{blocks::BlockDatabase, fonts::load_font},
    config::{
        EngineConfig, create_window_attributes, get_engine_config, update_engine_config_file,
    },
    game_window::GameWindow,
    voxels::{coord::WorldPos, world::World},
    worldgen::{generate_noise_world, text_generator::draw_text},
};

pub struct Application {
    pub game_window: Option<GameWindow>,
    engine_config: EngineConfig,
    config_debouncer: EventDebouncer<EngineConfig>,
    world: World,
    block_database: Arc<BlockDatabase>,
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

        let font = load_font(
            PathBuf::try_from("assets/fonts").unwrap().as_path(),
            "custom",
        )
        .expect("Failed to load font");

        let mut block_database = BlockDatabase::new();
        block_database
            .load_all_blocks()
            .expect("Failed to load block definitions");
        let block_database = Arc::new(block_database);

        let world = generate_noise_world(8);
        draw_text(&world, WorldPos::new(0, 16, 0), &font, "Hello, world!");

        Application {
            game_window: None,
            engine_config,
            config_debouncer,
            world,
            block_database,
        }
    }
}

impl ApplicationHandler<GameWindow> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = create_window_attributes(&self.engine_config);
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        let mut game_window =
            pollster::block_on(GameWindow::new(window, self.block_database.clone()))
                .expect("Failed to create the game window");

        game_window
            .world_renderer
            .material_manager
            .load_all_materials(self.block_database.iter_blocks())
            .expect("Failed to load block materials");
        game_window.world_renderer.create_all_chunks(&self.world);

        self.game_window = Some(game_window);
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
                game_window.resize(size);
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
