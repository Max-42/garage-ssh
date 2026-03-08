use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

/// Application configuration loaded from Home Assistant options.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub ssh_port: u16,
    pub webhook_url: String,
    pub home_latitude: f64,
    pub home_longitude: f64,
    pub geofence_radius_km: f64,
    pub geofence_override_timeout_sec: u64,
    pub tofu_timeout_sec: u64,
    pub untrusted_key_retention_days: u64,
    pub expected_json_version: String,
    pub log_level: String,
    /// PEM-encoded host key for migration/backup convenience.
    /// If empty, a key is auto-generated and saved back into config.
    #[serde(default)]
    pub host_key_pem: String,
}

impl AppConfig {
    /// Load configuration from the HA options.json file
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path, e))?;
        let config: AppConfig = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;

        info!("Configuration loaded from {}", path);
        Ok(config)
    }

    /// Reload configuration (for live updates from HA UI)
    #[allow(dead_code)]
    pub fn reload(&mut self, path: &str) -> anyhow::Result<()> {
        let new_config = Self::load(path)?;
        *self = new_config;
        info!("Configuration reloaded");
        Ok(())
    }
}
