use std::{path::PathBuf, sync::Arc, time::Instant};

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
    gameplay::physics::world_collider::PhysicsWorld,
    voxels::{coord::WorldPos, world::World},
    worldgen::{generate_noise_world, text_generator::draw_text},
};

pub struct Application {
    pub game_window: Option<GameWindow>,
    engine_config: EngineConfig,
    config_debouncer: EventDebouncer<EngineConfig>,
    world: World,
    block_database: Arc<BlockDatabase>,
    physics: PhysicsWorld,
    last_update: Instant,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
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

        let font = load_font(PathBuf::from("assets/fonts").as_path(), "custom")
            .expect("Failed to load font");

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

        Application {
            game_window: None,
            engine_config,
            config_debouncer,
            world,
            block_database,
            physics,
            last_update: Instant::now(),
        }
    }

    fn update(&mut self) {
        let delta_time = self.last_update.elapsed().as_secs_f32();
        self.physics.update(delta_time);
        self.last_update = Instant::now();
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
            .texture_manager
            .load_all_textures(self.block_database.iter_blocks())
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.engine_config.window_size = Some((size.width, size.height));
                self.config_debouncer.put(self.engine_config.clone());
                self.game_window.as_mut().unwrap().resize(size);
            }
            WindowEvent::Moved(position) => {
                if self.game_window.as_ref().unwrap().is_minimized() {
                    return;
                }
                self.engine_config.window_position = Some((position.x, position.y));
                self.config_debouncer.put(self.engine_config.clone());
            }
            WindowEvent::RedrawRequested => {
                {
                    self.update();
                }
                self.game_window.as_mut().unwrap().render().unwrap();
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
                (KeyCode::F2, ElementState::Pressed) => {
                    self.game_window
                        .as_mut()
                        .unwrap()
                        .world_renderer
                        .camera
                        .toggle_ao();
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
