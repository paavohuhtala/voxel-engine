use std::sync::Arc;

use anyhow::Context;
use renderer::{renderer::Renderer, rendering::resolution::Resolution};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use engine::{
    EngineContext,
    config::config_manager::ConfigManager,
    game_loop::{Game, GameLoopTime},
};

use crate::config::ClientConfig;

pub struct ClientGame {
    should_exit: bool,
    renderer: Option<Renderer>,
    ctx: EngineContext,
    client_config: ConfigManager<ClientConfig>,
}

impl Game for ClientGame {
    #[profiling::function]
    fn update(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        self.ctx.physics.update(time.delta_time_s as f32);
        self.ctx.player.update(time);

        self.ctx
            .world
            .chunk_loader
            .update_camera_position(self.ctx.player.camera.get_current_chunk());

        self.ctx.world.update();

        // TODO: Reuse vec
        let mut chunks_to_mesh = Vec::new();
        self.ctx
            .world
            .get_chunks_ready_for_meshing(&mut chunks_to_mesh);

        for pos in chunks_to_mesh {
            self.renderer
                .as_mut()
                .unwrap()
                .world_renderer
                .mesh_generator
                .generate_chunk_mesh_async(&self.ctx.world, pos);
        }

        // Unload chunks
        let mut chunks_to_unload = Vec::new();
        self.ctx
            .world
            .get_chunks_ready_to_unload(&mut chunks_to_unload);

        for pos in chunks_to_unload {
            self.renderer
                .as_mut()
                .unwrap()
                .world_renderer
                .remove_chunk(pos);
        }

        self.renderer
            .as_mut()
            .unwrap()
            .update_camera(&self.ctx.player.camera, false);
        self.renderer.as_mut().unwrap().update(time);
        Ok(())
    }

    #[profiling::function]
    fn render(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        renderer.render(time)?;
        Ok(())
    }
}

impl ClientGame {
    pub fn new(engine_context: EngineContext, client_config: ConfigManager<ClientConfig>) -> Self {
        ClientGame {
            should_exit: false,
            renderer: None,
            ctx: engine_context,
            client_config,
        }
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    pub fn on_resumed(&mut self, window: Arc<Window>) {
        let mut renderer =
            pollster::block_on(Renderer::new(window, self.ctx.block_database.clone()))
                .context("Failed to create the renderer")
                .unwrap();

        // Copy initial camera state
        renderer.update_camera(&self.ctx.player.camera, true);

        // Load textures and create chunks (again)
        let block_database = self.ctx.block_database.clone();

        renderer
            .world_renderer
            .texture_manager
            .load_all_textures(block_database.iter_blocks())
            .expect("Failed to load block materials");
        renderer.world_renderer.create_all_chunks(&self.ctx.world);

        self.renderer = Some(renderer);
    }

    pub fn on_window_resized(&mut self, size: Resolution) {
        self.client_config.update_and_save(|config| {
            config.window_size = Some((size.width, size.height));
        });
        self.renderer.as_mut().unwrap().resize(size);
    }

    pub fn on_window_moved(&mut self, position: PhysicalPosition<i32>) {
        if self.renderer.as_ref().unwrap().is_minimized() {
            return;
        }
        self.client_config.update_and_save(|config| {
            config.window_position = Some((position.x, position.y));
        });
    }

    pub fn on_key_event(&mut self, key_event: &KeyEvent) {
        let KeyEvent {
            physical_key: PhysicalKey::Code(code),
            state: key_state,
            ..
        } = key_event
        else {
            return;
        };

        match (code, key_state) {
            (KeyCode::Escape, ElementState::Pressed) => {
                self.should_exit = true;
            }
            (KeyCode::F2, ElementState::Pressed) => {
                self.renderer
                    .as_mut()
                    .unwrap()
                    .world_renderer
                    .camera
                    .toggle_ao();
            }
            _ => {}
        }
    }
}
