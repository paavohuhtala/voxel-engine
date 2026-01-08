use std::sync::Arc;

use anyhow::Context;
use egui_wgpu::ScreenDescriptor;
use renderer::{
    renderer::Renderer,
    rendering::resolution::{PhysicalSizeExt, Resolution},
};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use engine::{
    config::config_manager::ConfigManager,
    game_loop::{Game, GameLoopTime},
};

use crate::{
    client_types::ClientEngineContext,
    config::ClientConfig,
    egui::{egui_instance::EguiInstance, world_stats::draw_world_stats_ui},
    fps_counter::FpsCounter,
};

pub struct ClientGame {
    should_exit: bool,
    pub renderer: Option<Renderer>,
    pub egui: Option<EguiInstance>,
    pub ctx: ClientEngineContext,
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

        Ok(())
    }

    fn before_render(&mut self, time: &GameLoopTime) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        let resolution = renderer.resolution();
        self.ctx.player.before_render(resolution.to_vec2());
        renderer.set_camera(&self.ctx.player.camera, time);

        if let Some(world) = &mut self.ctx.world {
            *world.chunk_loader.camera.write().unwrap() =
                renderer.world_renderer.camera.interpolated_camera.clone();

            // We probably shouldn't do this every frame, but it's fine for now
            world.chunk_loader.notify_camera_moved();
        }
    }

    #[profiling::function]
    fn render(&mut self, time: &GameLoopTime) -> anyhow::Result<()> {
        self.draw_egui();

        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };

        if let Some(world) = &self.ctx.world {
            renderer.world_renderer.sync_with_world(world);
        }

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
    pub fn new(
        engine_context: ClientEngineContext,
        client_config: ConfigManager<ClientConfig>,
    ) -> Self {
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

        if let Some(world) = &self.ctx.world {
            draw_world_stats_ui(
                &self.renderer.as_ref().unwrap().world_renderer,
                world,
                egui_renderer.ctx(),
            );
        }
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

        // renderer.world_renderer.create_all_chunks(&self.ctx.world);

        self.renderer = Some(renderer);
    }

    pub fn on_window_resized(&mut self, size: Resolution) {
        self.client_config.update_and_save(|config| {
            config.window_size = Some((size.width, size.height));
        });
        self.ctx.player.camera.update_matrices(size.to_vec2());
        self.renderer
            .as_mut()
            .unwrap()
            .set_camera_immediate(&self.ctx.player.camera);
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
            (KeyCode::F5, ElementState::Pressed) => {
                let world_renderer = &mut self.renderer.as_mut().unwrap().world_renderer;
                world_renderer.toggle_chunk_bounds();
            }
            _ => {}
        }
    }
}
