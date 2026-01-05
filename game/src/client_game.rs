use std::sync::Arc;

use anyhow::Context;
use egui_wgpu::ScreenDescriptor;
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

use crate::{
    config::ClientConfig,
    egui::{egui_instance::EguiInstance, world_stats::draw_world_stats_ui},
    fps_counter::FpsCounter,
};

pub struct ClientGame {
    should_exit: bool,
    renderer: Option<Renderer>,
    pub egui: Option<EguiInstance>,
    ctx: EngineContext,
    client_config: ConfigManager<ClientConfig>,
    fps_counter: FpsCounter,
}

impl Game for ClientGame {
    fn before_update(&mut self) {
        if let Some(egui_renderer) = &mut self.egui {
            egui_renderer.begin_frame();
        }
        self.fps_counter.tick();
    }

    #[profiling::function]
    fn update(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        // TODO: Re-enable physics when we start using it for something
        // self.ctx.physics.update(time.delta_time_s as f32);
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

        Ok(())
    }

    #[profiling::function]
    fn render(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        if let Some(renderer) = &mut self.renderer {
            renderer.set_camera(&self.ctx.player.camera);
        }

        self.draw_egui();

        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };

        let egui_renderer = &mut self.egui;

        renderer.render(
            time,
            Some(
                |encoder: &mut egui_wgpu::wgpu::CommandEncoder,
                 view: &egui_wgpu::wgpu::TextureView,
                 descriptor: renderer::renderer::ScreenDescriptor| {
                    if let Some(egui_instance) = egui_renderer {
                        egui_instance.end_frame_and_draw(
                            encoder,
                            view,
                            ScreenDescriptor {
                                size_in_pixels: descriptor.size_in_pixels,
                                pixels_per_point: descriptor.pixels_per_point,
                            },
                        );
                    }
                },
            ),
        )?;

        Ok(())
    }
}

impl ClientGame {
    pub fn new(engine_context: EngineContext, client_config: ConfigManager<ClientConfig>) -> Self {
        ClientGame {
            should_exit: false,
            renderer: None,
            egui: None,
            ctx: engine_context,
            client_config,
            fps_counter: FpsCounter::new(),
        }
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn draw_egui(&mut self) {
        let Some(egui_renderer) = &mut self.egui else {
            return;
        };

        self.fps_counter.draw_ui(egui_renderer.ctx());

        draw_world_stats_ui(
            &self.renderer.as_ref().unwrap().world_renderer,
            &self.ctx.world,
            egui_renderer.ctx(),
        );
    }

    pub fn on_resumed(&mut self, window: Arc<Window>) {
        let mut renderer = pollster::block_on(Renderer::new(
            window.clone(),
            self.ctx.block_database.clone(),
        ))
        .context("Failed to create the renderer")
        .unwrap();

        let egui_renderer = EguiInstance::new(window.clone(), &renderer.device, &renderer.queue);
        self.egui = Some(egui_renderer);

        renderer.set_camera_immediate(&self.ctx.player.camera);

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
            (KeyCode::F3, ElementState::Pressed) => {
                self.ctx.player.should_move_camera = !self.ctx.player.should_move_camera;
            }
            (KeyCode::F4, ElementState::Pressed) => {
                let world_renderer = &mut self.renderer.as_mut().unwrap().world_renderer;
                world_renderer.camera.toggle_face_colors();
            }
            _ => {}
        }
    }
}
