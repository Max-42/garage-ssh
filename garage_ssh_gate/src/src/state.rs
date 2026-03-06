use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use uuid::Uuid;
use tracing::{info, warn};
use fd_lock::RwLock;

/// Persistent application state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// Trusted SSH keys
    pub trusted_keys: Vec<TrustedKey>,
    /// Untrusted/pending SSH keys
    pub untrusted_keys: Vec<UntrustedKey>,
    /// Connection attempt log
    pub connection_log: Vec<ConnectionLogEntry>,
    /// TOFU mode state
    pub tofu: TofuState,
    /// Geofence override cache: fingerprint -> timestamp of first attempt
    pub geofence_overrides: HashMap<String, DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    pub id: Uuid,
    pub fingerprint: String,
    pub public_key: String,
    pub key_type: String,
    pub username: String,
    pub device_name: String,
    pub trusted_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub use_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UntrustedKey {
    pub id: Uuid,
    pub fingerprint: String,
    pub public_key: String,
    pub key_type: String,
    pub ssh_username: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub attempt_count: u64,
    /// Last client payload (sanitized)
    pub last_payload: Option<ClientPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientPayload {
    pub time: Option<String>,
    pub device_model: Option<String>,
    pub device_name: Option<String>,
    pub device_hostname: Option<String>,
    pub device_os: Option<String>,
    pub device_version: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub version: Option<String>,
    pub raw_sanitized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionLogEntry {
    pub timestamp: DateTime<Utc>,
    pub fingerprint: String,
    pub ssh_username: String,
    pub result: ConnectionResult,
    pub client_ip: String,
    pub payload: Option<ClientPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionResult {
    Success,
    UntrustedKey,
    InvalidJson(String),
    VersionMismatch { expected: String, got: String },
    OutsideGeofence { distance_km: f64 },
    GeofenceOverrideSuccess { distance_km: f64 },
    TofuTrusted,
    Error(String),
}

impl std::fmt::Display for ConnectionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionResult::Success => write!(f, "SUCCESS"),
            ConnectionResult::UntrustedKey => write!(f, "UNTRUSTED_KEY"),
            ConnectionResult::InvalidJson(e) => write!(f, "INVALID_JSON: {}", e),
            ConnectionResult::VersionMismatch { expected, got } => {
                write!(f, "VERSION_MISMATCH: expected={}, got={}", expected, got)
            }
            ConnectionResult::OutsideGeofence { distance_km } => {
                write!(f, "OUTSIDE_GEOFENCE: {:.1}km away", distance_km)
            }
            ConnectionResult::GeofenceOverrideSuccess { distance_km } => {
                write!(f, "GEOFENCE_OVERRIDE_SUCCESS: {:.1}km away", distance_km)
            }
            ConnectionResult::TofuTrusted => write!(f, "TOFU_TRUSTED"),
            ConnectionResult::Error(e) => write!(f, "ERROR: {}", e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TofuState {
    pub active: bool,
    pub activated_at: Option<DateTime<Utc>>,
    pub timeout_sec: u64,
}

impl Default for TofuState {
    fn default() -> Self {
        Self {
            active: false,
            activated_at: None,
            timeout_sec: 45,
        }
    }
}

impl TofuState {
    /// Check if TOFU mode is currently active
    pub fn is_active(&self) -> bool {
        if !self.active {
            return false;
        }
        if let Some(activated_at) = self.activated_at {
            let elapsed = Utc::now().signed_duration_since(activated_at);
            elapsed.num_seconds() < self.timeout_sec as i64
        } else {
            false
        }
    }
    
    /// Activate TOFU mode for the configured duration
    pub fn activate(&mut self, timeout_sec: u64) {
        self.active = true;
        self.activated_at = Some(Utc::now());
        self.timeout_sec = timeout_sec;
        info!("TOFU mode activated for {} seconds", timeout_sec);
    }
    
    /// Deactivate TOFU mode
    pub fn deactivate(&mut self) {
        self.active = false;
        self.activated_at = None;
        info!("TOFU mode deactivated");
    }
}

impl AppState {
    /// Create a new empty state
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
            untrusted_keys: Vec::new(),
            connection_log: Vec::new(),
            tofu: TofuState::default(),
            geofence_overrides: HashMap::new(),
        }
    }
    
    /// Load state from disk or create new
    pub fn load_or_create(path: &str) -> anyhow::Result<Self> {
        if std::path::Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            let state: AppState = serde_json::from_str(&content)
                .map_err(|e| {
                    warn!("Failed to parse state file, creating new: {}", e);
                    e
                })
                .unwrap_or_else(|_| AppState::new());
            info!("State loaded from {}", path);
            Ok(state)
        } else {
            info!("No state file found, creating new state");
            Ok(AppState::new())
        }
    }
    
    /// Save state to disk with file locking to prevent lost updates
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = format!("{}.tmp", path);
        
        // Write to temp file first
        let tmp_file = fs::File::create(&tmp_path)?;
        let mut lock = RwLock::new(tmp_file);
        {
            let mut guard = lock.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            guard.write_all(json.as_bytes())?;
            guard.flush()?;
        }
        
        // Atomic rename
        fs::rename(&tmp_path, path)?;
        Ok(())
    }
    
    /// Find a trusted key by fingerprint
    pub fn find_trusted_key(&self, fingerprint: &str) -> Option<&TrustedKey> {
        self.trusted_keys.iter().find(|k| k.fingerprint == fingerprint)
    }
    
    /// Find a trusted key by fingerprint (mutable)
    pub fn find_trusted_key_mut(&mut self, fingerprint: &str) -> Option<&mut TrustedKey> {
        self.trusted_keys.iter_mut().find(|k| k.fingerprint == fingerprint)
    }
    
    /// Find an untrusted key by fingerprint
    #[allow(dead_code)]
    pub fn find_untrusted_key(&self, fingerprint: &str) -> Option<&UntrustedKey> {
        self.untrusted_keys.iter().find(|k| k.fingerprint == fingerprint)
    }
    
    /// Record a new untrusted key or update existing
    pub fn record_untrusted_key(
        &mut self,
        fingerprint: String,
        public_key: String,
        key_type: String,
        ssh_username: String,
        payload: Option<ClientPayload>,
    ) {
        let now = Utc::now();
        if let Some(existing) = self.untrusted_keys.iter_mut().find(|k| k.fingerprint == fingerprint) {
            existing.last_seen = now;
            existing.attempt_count += 1;
            existing.ssh_username = ssh_username;
            if payload.is_some() {
                existing.last_payload = payload;
            }
        } else {
            self.untrusted_keys.push(UntrustedKey {
                id: Uuid::new_v4(),
                fingerprint,
                public_key,
                key_type,
                ssh_username,
                first_seen: now,
                last_seen: now,
                attempt_count: 1,
                last_payload: payload,
            });
        }
    }
    
    /// Trust a key by fingerprint - move from untrusted to trusted
    pub fn trust_key(
        &mut self,
        fingerprint: &str,
        username: String,
        device_name: String,
    ) -> anyhow::Result<()> {
        let untrusted = self.untrusted_keys
            .iter()
            .position(|k| k.fingerprint == fingerprint)
            .ok_or_else(|| anyhow::anyhow!("Key not found: {}", fingerprint))?;
        
        let key = self.untrusted_keys.remove(untrusted);
        
        self.trusted_keys.push(TrustedKey {
            id: Uuid::new_v4(),
            fingerprint: key.fingerprint,
            public_key: key.public_key,
            key_type: key.key_type,
            username,
            device_name,
            trusted_at: Utc::now(),
            last_used: None,
            use_count: 0,
        });
        
        info!("Key trusted: {} ({})", key.ssh_username, key.id);
        Ok(())
    }
    
    /// Revoke trust for a key by fingerprint
    pub fn revoke_key(&mut self, fingerprint: &str) -> anyhow::Result<()> {
        let pos = self.trusted_keys
            .iter()
            .position(|k| k.fingerprint == fingerprint)
            .ok_or_else(|| anyhow::anyhow!("Trusted key not found: {}", fingerprint))?;
        
        let key = self.trusted_keys.remove(pos);
        info!("Key revoked: {} ({})", key.username, key.fingerprint);
        Ok(())
    }
    
    /// Delete an untrusted key
    pub fn delete_untrusted_key(&mut self, fingerprint: &str) -> anyhow::Result<()> {
        let pos = self.untrusted_keys
            .iter()
            .position(|k| k.fingerprint == fingerprint)
            .ok_or_else(|| anyhow::anyhow!("Untrusted key not found: {}", fingerprint))?;
        
        self.untrusted_keys.remove(pos);
        Ok(())
    }
    
    /// Add a connection log entry
    pub fn log_connection(&mut self, entry: ConnectionLogEntry) {
        info!(
            "Connection: {} from {} [{}] - {}",
            entry.ssh_username, entry.client_ip, entry.fingerprint, entry.result
        );
        self.connection_log.push(entry);
        
        // Keep last 10000 log entries
        if self.connection_log.len() > 10000 {
            self.connection_log.drain(..self.connection_log.len() - 10000);
        }
    }
    
    /// Remove untrusted keys older than the retention period
    pub fn cleanup_expired_keys(&mut self, retention_days: u64) {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        let before = self.untrusted_keys.len();
        self.untrusted_keys.retain(|k| k.last_seen > cutoff);
        let removed = before - self.untrusted_keys.len();
        if removed > 0 {
            info!("Cleaned up {} expired untrusted keys", removed);
        }
    }
    
    /// Clean up expired geofence overrides
    pub fn cleanup_geofence_overrides(&mut self, timeout_sec: u64) {
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_sec as i64);
        self.geofence_overrides.retain(|_, ts| *ts > cutoff);
    }
    
    /// Check and handle geofence override
    pub fn check_geofence_override(&mut self, fingerprint: &str, timeout_sec: u64) -> bool {
        self.cleanup_geofence_overrides(timeout_sec);
        
        if let Some(first_attempt) = self.geofence_overrides.get(fingerprint) {
            let elapsed = Utc::now().signed_duration_since(*first_attempt);
            if elapsed.num_seconds() < timeout_sec as i64 {
                // Second attempt within timeout - allow
                self.geofence_overrides.remove(fingerprint);
                return true;
            }
        }
        
        // First attempt or expired - record it
        self.geofence_overrides.insert(fingerprint.to_string(), Utc::now());
        false
    }
}
