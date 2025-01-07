use std::path::Path;

mod error;
mod monitor_config;
mod network_config;
mod trigger_config;

/// Common interface for loading configuration files
pub trait ConfigLoader: Sized {
    fn load_all<T>(path: Option<&Path>) -> Result<T, error::ConfigError>
    where
        T: FromIterator<(String, Self)>;

    fn load_from_path(path: &Path) -> Result<Self, error::ConfigError>;

    fn validate(&self) -> Result<(), String>;

    fn is_json_file(path: &Path) -> bool {
        path.extension()
            .map(|ext| ext.to_string_lossy().to_lowercase() == "json")
            .unwrap_or(false)
    }
}
