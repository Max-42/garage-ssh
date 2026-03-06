use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use russh::server::{Auth, Handler, Msg, Server as RusshServer, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodSet};
use russh_keys::key::{KeyPair, PublicKey};
use sha2::{Sha256, Digest};
use tokio::sync::RwLock;
use tracing::{info, warn, error};

use crate::config::AppConfig;
use crate::geo;
use crate::sanitize;
use crate::state::{AppState, ClientPayload, ConnectionLogEntry, ConnectionResult};
use crate::webhook;

const STATE_PATH: &str = "/data/state.json";

/// Load host key from config PEM, from file fallback, or generate a new one.
/// If a new key is generated it is written back to options.json so that
/// backups/migrations automatically include the host key.
pub fn load_or_generate_host_key(
    config: &mut crate::config::AppConfig,
    file_path: &str,
    options_path: &str,
) -> anyhow::Result<KeyPair> {
    // 1) Try config PEM first (migration / backup restore)
    if !config.host_key_pem.is_empty() {
        info!("Loading host key from add-on configuration (host_key_pem)");
        let key = russh_keys::decode_secret_key(&config.host_key_pem, None)?;
        // Also persist to file for fast subsequent starts
        std::fs::write(file_path, config.host_key_pem.as_bytes())?;
        return Ok(key);
    }

    // 2) Try loading from file (normal operation)
    if std::path::Path::new(file_path).exists() {
        info!("Loading existing host key from {}", file_path);
        let pem = std::fs::read_to_string(file_path)?;
        let key = russh_keys::decode_secret_key(&pem, None)?;
        // Save into config for backup convenience
        save_host_key_to_config(&pem, config, options_path)?;
        return Ok(key);
    }

    // 3) Generate new key
    info!("Generating new ED25519 host key");
    let key = KeyPair::generate_ed25519().expect("Failed to generate ED25519 key");
    let pem = russh_keys::encode_pkcs8_pem(&key)?;
    std::fs::write(file_path, pem.as_bytes())?;
    save_host_key_to_config(&pem, config, options_path)?;
    info!("Host key generated and saved to config for backup");
    Ok(key)
}

/// Write the host key PEM back into options.json so HA backups include it.
fn save_host_key_to_config(
    pem: &str,
    config: &mut crate::config::AppConfig,
    options_path: &str,
) -> anyhow::Result<()> {
    config.host_key_pem = pem.to_string();
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(options_path, json.as_bytes())?;
    info!("Host key PEM saved to add-on configuration for backup/migration");
    Ok(())
}

/// Compute SHA-256 fingerprint of a public key
fn compute_fingerprint(key: &PublicKey) -> String {
    let key_bytes = key.public_key_bytes();
    let mut hasher = Sha256::new();
    hasher.update(&key_bytes);
    let result = hasher.finalize();
    format!("SHA256:{}", hex::encode(result))
}

/// Get human-readable key type
fn key_type_string(key: &PublicKey) -> String {
    match key {
        PublicKey::Ed25519(_) => "ed25519".to_string(),
        PublicKey::RSA { .. } => "rsa".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Encode public key to authorized_keys format string
fn encode_public_key(key: &PublicKey) -> String {
    let key_bytes = key.public_key_bytes();
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&key_bytes);
    format!("{} {}", key_type_string(key), encoded)
}

struct GarageSSHServer {
    state: Arc<RwLock<AppState>>,
    config: Arc<RwLock<AppConfig>>,
}

impl RusshServer for GarageSSHServer {
    type Handler = GarageSSHHandler;
    
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let addr = peer_addr
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        info!("New SSH connection from {}", addr);
        
        GarageSSHHandler {
            state: self.state.clone(),
            config: self.config.clone(),
            client_ip: addr,
            authenticated_fingerprint: None,
            ssh_username: String::new(),
            public_key_str: String::new(),
            key_type: String::new(),
            stdin_data: Vec::new(),
            channel_id: None,
        }
    }
}

struct GarageSSHHandler {
    state: Arc<RwLock<AppState>>,
    config: Arc<RwLock<AppConfig>>,
    client_ip: String,
    authenticated_fingerprint: Option<String>,
    ssh_username: String,
    public_key_str: String,
    key_type: String,
    stdin_data: Vec<u8>,
    channel_id: Option<ChannelId>,
}

#[async_trait]
impl Handler for GarageSSHHandler {
    type Error = anyhow::Error;
    
    async fn auth_none(&mut self, user: &str) -> Result<Auth, Self::Error> {
        self.ssh_username = sanitize::sanitize_short_string(user);
        info!("Auth attempt (none) from user: {}", self.ssh_username);
        // Reject none auth - require publickey
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::PUBLICKEY),
        })
    }
    
    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        self.ssh_username = sanitize::sanitize_short_string(user);
        warn!("Password auth attempted by {} from {} - rejecting", self.ssh_username, self.client_ip);
        // Never accept passwords
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::PUBLICKEY),
        })
    }
    
    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.ssh_username = sanitize::sanitize_short_string(user);
        let fingerprint = compute_fingerprint(public_key);
        self.public_key_str = encode_public_key(public_key);
        self.key_type = key_type_string(public_key);
        
        info!(
            "Public key auth from user='{}' fingerprint={} type={} ip={}",
            self.ssh_username, fingerprint, self.key_type, self.client_ip
        );
        
        // Always accept the key for authentication purposes.
        // We handle authorization (trusted/untrusted) after the channel is opened
        // and we receive the stdin data.
        self.authenticated_fingerprint = Some(fingerprint);
        
        Ok(Auth::Accept)
    }
    
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channel_id = Some(channel.id());
        info!("Channel opened for {}", self.ssh_username);
        Ok(true)
    }
    
    async fn exec_request(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        // Some clients may send data via exec
        let data_str = String::from_utf8_lossy(data);
        info!("Exec request from {}: {} bytes", self.ssh_username, data.len());
        self.stdin_data.extend_from_slice(data);
        
        // Process the connection
        self.process_connection(channel_id, session).await?;
        Ok(true)
    }
    
    async fn data(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Accumulate stdin data (limit to 64KB to prevent abuse)
        if self.stdin_data.len() < 65536 {
            self.stdin_data.extend_from_slice(data);
        }
        Ok(())
    }
    
    async fn shell_request(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        info!("Shell request from {}", self.ssh_username);
        // Wait a moment for data, then process
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        self.process_connection(channel_id, session).await?;
        Ok(true)
    }

    async fn channel_eof(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        info!("Channel EOF from {}", self.ssh_username);
        // Process when client indicates end of data
        if self.authenticated_fingerprint.is_some() {
            self.process_connection(channel_id, session).await?;
        }
        Ok(())
    }
}

impl GarageSSHHandler {
    async fn process_connection(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> anyhow::Result<()> {
        let fingerprint = match &self.authenticated_fingerprint {
            Some(fp) => fp.clone(),
            None => {
                self.send_response(channel_id, session, "ERROR: No key authenticated\n").await;
                return Ok(());
            }
        };
        
        // Prevent double-processing
        let already_fingerprint = self.authenticated_fingerprint.take();
        if already_fingerprint.is_none() {
            return Ok(());
        }
        
        let config = self.config.read().await;
        let mut state = self.state.write().await;
        
        // Parse stdin JSON payload
        let stdin_str = String::from_utf8_lossy(&self.stdin_data).to_string();
        let payload = self.parse_client_payload(&stdin_str, &config);
        
        // Always record the key (even if JSON is invalid) for Android support etc.
        state.record_untrusted_key(
            fingerprint.clone(),
            self.public_key_str.clone(),
            self.key_type.clone(),
            self.ssh_username.clone(),
            payload.as_ref().ok().and_then(|p| p.clone()),
        );
        
        // Check if payload parsing had errors
        let (parsed_payload, json_error) = match payload {
            Ok(p) => (p, None),
            Err(e) => (None, Some(e)),
        };
        
        // Handle JSON validation errors
        if let Some(error_msg) = &json_error {
            let result = if error_msg.contains("version mismatch") {
                ConnectionResult::VersionMismatch {
                    expected: config.expected_json_version.clone(),
                    got: error_msg.clone(),
                }
            } else {
                ConnectionResult::InvalidJson(error_msg.clone())
            };
            
            state.log_connection(ConnectionLogEntry {
                timestamp: Utc::now(),
                fingerprint: fingerprint.clone(),
                ssh_username: self.ssh_username.clone(),
                result,
                client_ip: self.client_ip.clone(),
                payload: None,
            });
            
            state.save(STATE_PATH)?;
            self.send_response(
                channel_id,
                session,
                &format!("FAIL: {}\n", error_msg),
            ).await;
            session.close(channel_id);
            return Ok(());
        }
        
        // Check if key is trusted
        let is_trusted = state.find_trusted_key(&fingerprint).is_some();
        
        // Check if TOFU mode is active
        let tofu_active = state.tofu.is_active();
        let tofu_activated_at = state.tofu.activated_at;
        
        if !is_trusted && tofu_active {
            // TOFU: only auto-trust keys that are genuinely NEW connections,
            // NOT keys that were already pending/untrusted before TOFU was activated.
            let key_existed_before_tofu = state.untrusted_keys.iter().any(|k| {
                k.fingerprint == fingerprint
                    && tofu_activated_at
                        .map(|tofu_at| k.first_seen < tofu_at)
                        .unwrap_or(true)
            });
            
            if key_existed_before_tofu {
                // This key was already pending before TOFU – do NOT auto-trust
                info!(
                    "TOFU active but key {} was already pending before activation, skipping auto-trust",
                    fingerprint
                );
                state.log_connection(ConnectionLogEntry {
                    timestamp: Utc::now(),
                    fingerprint: fingerprint.clone(),
                    ssh_username: self.ssh_username.clone(),
                    result: ConnectionResult::UntrustedKey,
                    client_ip: self.client_ip.clone(),
                    payload: parsed_payload.clone(),
                });
                
                state.save(STATE_PATH)?;
                self.send_response(
                    channel_id,
                    session,
                    "FAIL: Key was already pending before TOFU mode. Please ask the admin to trust it manually.\n",
                ).await;
                session.close(channel_id);
                return Ok(());
            }
            
            // Genuinely new key during TOFU window – auto-trust
            let device_name = parsed_payload
                .as_ref()
                .and_then(|p| p.device_name.clone())
                .unwrap_or_else(|| "Unknown Device".to_string());
            
            state.trust_key(
                &fingerprint,
                self.ssh_username.clone(),
                device_name,
            )?;
            
            state.log_connection(ConnectionLogEntry {
                timestamp: Utc::now(),
                fingerprint: fingerprint.clone(),
                ssh_username: self.ssh_username.clone(),
                result: ConnectionResult::TofuTrusted,
                client_ip: self.client_ip.clone(),
                payload: parsed_payload.clone(),
            });
            
            info!("Key auto-trusted via TOFU: {}", fingerprint);
            // Now it's trusted, continue with geofence check below
        } else if !is_trusted && !tofu_active {
            // Key not trusted and TOFU not active
            state.log_connection(ConnectionLogEntry {
                timestamp: Utc::now(),
                fingerprint: fingerprint.clone(),
                ssh_username: self.ssh_username.clone(),
                result: ConnectionResult::UntrustedKey,
                client_ip: self.client_ip.clone(),
                payload: parsed_payload.clone(),
            });
            
            state.save(STATE_PATH)?;
            self.send_response(
                channel_id,
                session,
                "FAIL: Key not trusted. Please ask the administrator to trust your key.\n",
            ).await;
            session.close(channel_id);
            return Ok(());
        }
        
        // Key is trusted (either previously or via TOFU) - check geofence
        // NOTE: Geofencing is a CLIENT-SIDE convenience feature only!
        // The position comes from the client and can be trivially spoofed.
        // It does NOT provide any real server-side security.
        // The only real authentication is the SSH private key.
        if let Some(ref payload) = parsed_payload {
            if let (Some(lat), Some(lon)) = (payload.latitude, payload.longitude) {
                if config.home_latitude != 0.0 || config.home_longitude != 0.0 {
                    let (within, distance_km) = geo::is_within_geofence(
                        config.home_latitude,
                        config.home_longitude,
                        lat,
                        lon,
                        config.geofence_radius_km,
                    );
                    
                    if !within {
                        // Check if this is a geofence override (second attempt)
                        let override_success = state.check_geofence_override(
                            &fingerprint,
                            config.geofence_override_timeout_sec,
                        );
                        
                        if override_success {
                            info!(
                                "Geofence override accepted for {} ({:.1}km away)",
                                fingerprint, distance_km
                            );
                            state.log_connection(ConnectionLogEntry {
                                timestamp: Utc::now(),
                                fingerprint: fingerprint.clone(),
                                ssh_username: self.ssh_username.clone(),
                                result: ConnectionResult::GeofenceOverrideSuccess { distance_km },
                                client_ip: self.client_ip.clone(),
                                payload: parsed_payload.clone(),
                            });
                            // Continue to success
                        } else {
                            state.log_connection(ConnectionLogEntry {
                                timestamp: Utc::now(),
                                fingerprint: fingerprint.clone(),
                                ssh_username: self.ssh_username.clone(),
                                result: ConnectionResult::OutsideGeofence { distance_km },
                                client_ip: self.client_ip.clone(),
                                payload: parsed_payload.clone(),
                            });
                            
                            state.save(STATE_PATH)?;
                            self.send_response(
                                channel_id,
                                session,
                                &format!(
                                    "FAIL: You are {:.1} km away from the garage (max: {} km).\n\
                                    If this is intentional, run the shortcut again within {} seconds to override.\n",
                                    distance_km,
                                    config.geofence_radius_km,
                                    config.geofence_override_timeout_sec
                                ),
                            ).await;
                            session.close(channel_id);
                            return Ok(());
                        }
                    }
                }
            }
        }
        
        // Update trusted key usage
        if let Some(trusted) = state.find_trusted_key_mut(&fingerprint) {
            trusted.last_used = Some(Utc::now());
            trusted.use_count += 1;
        }
        
        // Log success
        state.log_connection(ConnectionLogEntry {
            timestamp: Utc::now(),
            fingerprint: fingerprint.clone(),
            ssh_username: self.ssh_username.clone(),
            result: ConnectionResult::Success,
            client_ip: self.client_ip.clone(),
            payload: parsed_payload,
        });
        
        state.save(STATE_PATH)?;
        
        // Fire webhook
        let webhook_url = config.webhook_url.clone();
        drop(config);
        drop(state);
        
        match webhook::fire_webhook(&webhook_url).await {
            Ok(()) => {
                self.send_response(channel_id, session, "SUCCESS: Garage door opening!\n").await;
            }
            Err(e) => {
                error!("Webhook failed: {}", e);
                self.send_response(
                    channel_id,
                    session,
                    &format!("FAIL: Webhook error: {}\n", e),
                ).await;
            }
        }
        
        session.close(channel_id);
        Ok(())
    }
    
    fn parse_client_payload(
        &self,
        stdin_str: &str,
        config: &AppConfig,
    ) -> Result<Option<ClientPayload>, String> {
        let trimmed = stdin_str.trim();
        if trimmed.is_empty() {
            // No payload sent (e.g., Android client)
            return Ok(None);
        }
        
        // Parse JSON
        let json: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| format!("Invalid JSON: {}", e))?;
        
        // Sanitize the entire JSON tree
        let sanitized = sanitize::sanitize_json_value(&json);
        
        // Check version
        if let Some(version) = sanitized.get("version").and_then(|v| v.as_str()) {
            if version != config.expected_json_version {
                return Err(format!(
                    "version mismatch: expected '{}', got '{}'",
                    config.expected_json_version, version
                ));
            }
        }
        
        // Extract fields
        let time = sanitized
            .get("time")
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let device = sanitized.get("device");
        let device_model = device
            .and_then(|d| d.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let device_name = device
            .and_then(|d| d.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let device_hostname = device
            .and_then(|d| d.get("hostname"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let device_os = device
            .and_then(|d| d.get("os"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let device_version = device
            .and_then(|d| d.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let position = sanitized.get("position");
        let latitude = position
            .and_then(|p| p.get("latitude"))
            .and_then(|v| v.as_f64());
        let longitude = position
            .and_then(|p| p.get("longitude"))
            .and_then(|v| v.as_f64());
        let altitude = position
            .and_then(|p| p.get("altitude"))
            .and_then(|v| v.as_f64());
        
        let version = sanitized
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        Ok(Some(ClientPayload {
            time,
            device_model,
            device_name,
            device_hostname,
            device_os,
            device_version,
            latitude,
            longitude,
            altitude,
            version,
            raw_sanitized: serde_json::to_string(&sanitized).unwrap_or_default(),
        }))
    }
    
    async fn send_response(
        &self,
        channel_id: ChannelId,
        session: &mut Session,
        msg: &str,
    ) {
        let _ = session.data(channel_id, CryptoVec::from(msg.as_bytes()));
    }
}

/// Run the SSH server
pub async fn run(
    host_key: KeyPair,
    state: Arc<RwLock<AppState>>,
    config: Arc<RwLock<AppConfig>>,
) -> anyhow::Result<()> {
    let ssh_port = {
        let cfg = config.read().await;
        cfg.ssh_port
    };
    
    let russh_config = russh::server::Config {
        methods: MethodSet::PUBLICKEY,
        keys: vec![host_key],
        ..Default::default()
    };
    
    let mut server = GarageSSHServer {
        state,
        config,
    };
    
    let addr = format!("0.0.0.0:{}", ssh_port);
    info!("SSH server listening on {}", addr);
    
    server
        .run_on_address(Arc::new(russh_config), &addr)
        .await?;
    
    Ok(())
}
