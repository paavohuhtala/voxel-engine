use engine::config::config_manager::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RendererConfig {
    pub enable_vsync: bool,
}

impl Config for RendererConfig {
    fn get_path() -> &'static str {
        "renderer.ron"
    }
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self { enable_vsync: true }
    }
}
