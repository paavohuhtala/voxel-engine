use std::{io::Write, path::Path};

use serde::{Deserialize, Serialize};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position, Size},
    window::WindowAttributes,
};

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineConfig {
    pub window_size: Option<(u32, u32)>,
    pub window_position: Option<(i32, i32)>,
}

impl EngineConfig {
    pub fn is_valid(&self) -> bool {
        match self.window_size {
            Some((width, height)) if width == 0 || height == 0 => {
                return false;
            }
            _ => {}
        }

        true
    }
}

const ENGINE_CONFIG_FILE: &str = "engine.ron";

pub fn get_engine_config() -> anyhow::Result<EngineConfig> {
    let engine_config_path = Path::new(ENGINE_CONFIG_FILE);
    if engine_config_path.exists() {
        let config_data = std::fs::read_to_string(engine_config_path)?;
        let config: EngineConfig = ron::from_str(&config_data)?;
        Ok(config)
    } else {
        Ok(EngineConfig::default())
    }
}

pub fn update_engine_config_file(config: &EngineConfig) -> anyhow::Result<()> {
    if !config.is_valid() {
        return Ok(());
    }

    let engine_config_path = Path::new(ENGINE_CONFIG_FILE);
    let mut writer = std::fs::File::create(engine_config_path)?;
    let serialized = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())?;
    writer.write_all(serialized.as_bytes())?;
    Ok(())
}

pub fn create_window_attributes(config: &EngineConfig) -> WindowAttributes {
    let base_attributes = WindowAttributes::default().with_active(false);

    if let EngineConfig {
        window_position: Some(position),
        window_size: Some(size),
    } = config
    {
        base_attributes
            .with_inner_size(Size::Physical(PhysicalSize {
                width: size.0,
                height: size.1,
            }))
            .with_position(Position::Physical(PhysicalPosition {
                x: position.0,
                y: position.1,
            }))
    } else {
        base_attributes
    }
}
