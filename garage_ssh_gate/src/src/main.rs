mod config;
mod state;
mod ssh_server;
mod web_server;
mod webhook;
mod geo;
mod sanitize;
mod logging;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use config::AppConfig;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration from HA options
    let config = AppConfig::load("/data/options.json")?;
    
    // Initialize logging
    logging::init(&config.log_level);
    
    info!("Garage SSH Gate starting...");
    info!("SSH port: {}", config.ssh_port);
    info!("Geofence radius: {} km", config.geofence_radius_km);
    
    // Load or create persistent state
    let state = AppState::load_or_create("/data/state.json")?;
    let shared_state = Arc::new(RwLock::new(state));
    let shared_config = Arc::new(RwLock::new(config));
    
    // Clean up expired untrusted keys on startup
    {
        let config = shared_config.read().await;
        let mut state = shared_state.write().await;
        state.cleanup_expired_keys(config.untrusted_key_retention_days);
        state.save("/data/state.json")?;
    }
    
    // Generate or load host key (saved into config for backup/migration)
    let host_key = {
        let mut config = shared_config.write().await;
        ssh_server::load_or_generate_host_key(&mut config, "/data/host_key_ed25519", "/data/options.json")?
    };
    
    // Start SSH server
    let ssh_state = shared_state.clone();
    let ssh_config = shared_config.clone();
    let ssh_handle = tokio::spawn(async move {
        if let Err(e) = ssh_server::run(host_key, ssh_state, ssh_config).await {
            error!("SSH server error: {}", e);
        }
    });
    
    // Start web server (ingress)
    let web_state = shared_state.clone();
    let web_config = shared_config.clone();
    let web_handle = tokio::spawn(async move {
        if let Err(e) = web_server::run(web_state, web_config).await {
            error!("Web server error: {}", e);
        }
    });
    
    info!("Garage SSH Gate fully started");
    
    tokio::select! {
        r = ssh_handle => {
            error!("SSH server exited: {:?}", r);
        }
        r = web_handle => {
            error!("Web server exited: {:?}", r);
        }
    }
    
    Ok(())
}
