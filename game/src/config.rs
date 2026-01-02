use engine::config::config_manager::Config;
use serde::{Deserialize, Serialize};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position, Size},
    window::WindowAttributes,
};

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientConfig {
    pub window_size: Option<(u32, u32)>,
    pub window_position: Option<(i32, i32)>,
}

impl Config for ClientConfig {
    fn get_path() -> &'static str {
        "client.ron"
    }

    fn is_valid(&self) -> bool {
        match self.window_size {
            Some((width, height)) if width == 0 || height == 0 => {
                return false;
            }
            _ => {}
        }

        true
    }
}

impl ClientConfig {
    pub fn create_window_attributes(&self) -> WindowAttributes {
        let base_attributes = WindowAttributes::default().with_active(false);

        if let (Some((width, height)), Some((x, y))) = (self.window_size, self.window_position) {
            base_attributes
                .with_inner_size(Size::Physical(PhysicalSize { width, height }))
                .with_position(Position::Physical(PhysicalPosition { x, y }))
        } else {
            base_attributes
        }
    }
}
