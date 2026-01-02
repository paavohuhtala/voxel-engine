use serde::{Deserialize, Serialize};

use crate::config::config_manager::Config;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineConfig {}

impl Config for EngineConfig {
    fn get_path() -> &'static str {
        "engine.ron"
    }
}
