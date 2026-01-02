use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::Context;
use debounce::EventDebouncer;
use log::warn;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

pub struct ConfigManager<T> {
    path: PathBuf,
    current: Arc<RwLock<T>>,
    debouncer: debounce::EventDebouncer<UpdateConfigEvent>,
}

const CONFIG_DEBOUNCE_DURATION_MS: u64 = 200;

pub trait Config:
    Sized + Default + Clone + Send + Sync + Serialize + for<'a> Deserialize<'a> + 'static
{
    fn get_path() -> &'static str;

    fn is_valid(&self) -> bool {
        true
    }

    fn create_manager() -> anyhow::Result<ConfigManager<Self>> {
        let mut manager = ConfigManager::new(PathBuf::from(Self::get_path()));
        manager
            .load_if_exists()
            .with_context(|| format!("Failed to load config from {}", Self::get_path()))?;
        Ok(manager)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct UpdateConfigEvent;

impl<T> ConfigManager<T>
where
    T: Config,
{
    pub fn new(path: PathBuf) -> Self {
        let current = Arc::new(RwLock::new(T::default()));
        let current_clone = current.clone();
        let path_clone = path.clone();

        let write_config = move |_event: UpdateConfigEvent| {
            let mut writer = File::create(&path_clone).unwrap();
            let config = current_clone.read().unwrap();

            if !config.is_valid() {
                warn!("Attempted to write invalid config to {:?}", &path_clone);
                return;
            }

            let serialized = ron::ser::to_string_pretty(&*config, PrettyConfig::default()).unwrap();
            writer.write_all(serialized.as_bytes()).unwrap();
        };

        Self {
            path,
            current,
            debouncer: EventDebouncer::new(
                Duration::from_millis(CONFIG_DEBOUNCE_DURATION_MS),
                write_config,
            ),
        }
    }

    pub fn get(&self) -> Arc<RwLock<T>> {
        self.current.clone()
    }

    pub fn load_if_exists(&mut self) -> anyhow::Result<()> {
        if self.path.exists() {
            let config_data = std::fs::read_to_string(&self.path)?;

            if config_data.is_empty() {
                return Ok(());
            }

            let config: T = ron::from_str(&config_data)
                .with_context(|| format!("Failed to parse config from {:?}", &self.path))?;
            // TODO: Handle invalid config gracefully
            self.current.write().unwrap().clone_from(&config);
        }
        Ok(())
    }

    pub fn update_and_save<F>(&self, update_fn: F)
    where
        F: FnOnce(&mut T),
    {
        {
            let mut config = self.current.write().unwrap();
            update_fn(&mut *config);
        }
        self.debouncer.put(UpdateConfigEvent);
    }
}
