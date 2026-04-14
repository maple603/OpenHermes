//! Persistent multi-credential pool for same-provider failover.
//!
//! Supports multiple API keys per provider with configurable selection
//! strategies (fill_first, round_robin, random, least_used), exhaustion
//! tracking with automatic cooldown, and file-based persistence.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const STATUS_OK: &str = "ok";
const STATUS_EXHAUSTED: &str = "exhausted";

/// Cooldown before retrying an exhausted credential (1 hour).
const EXHAUSTED_TTL_SECONDS: i64 = 3600;

/// Known environment variable names for seeding credentials.
const ENV_CREDENTIAL_MAP: &[(&str, &str)] = &[
    ("openai", "OPENAI_API_KEY"),
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("deepseek", "DEEPSEEK_API_KEY"),
    ("google", "GOOGLE_API_KEY"),
    ("mistral", "MISTRAL_API_KEY"),
    ("groq", "GROQ_API_KEY"),
    ("together", "TOGETHER_API_KEY"),
    ("fireworks", "FIREWORKS_API_KEY"),
    ("perplexity", "PERPLEXITY_API_KEY"),
    ("cohere", "COHERE_API_KEY"),
];

// ---------------------------------------------------------------------------
// Selection strategy
// ---------------------------------------------------------------------------

/// Strategy for selecting credentials from the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    /// Use the highest-priority credential until exhausted.
    FillFirst,
    /// Rotate through credentials in order.
    RoundRobin,
    /// Pick a random available credential.
    Random,
    /// Pick the credential with the fewest requests.
    LeastUsed,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self::FillFirst
    }
}

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

/// A single pooled credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PooledCredential {
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
    /// Unique identifier within the pool.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Authentication type: "api_key" or "oauth".
    pub auth_type: String,
    /// Selection priority (lower = preferred).
    pub priority: i32,
    /// How the credential was added: "env", "manual", "device_code".
    pub source: String,
    /// The API key or access token.
    pub access_token: String,
    /// Optional refresh token for OAuth credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Optional base URL override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Current status: "ok" or "exhausted".
    #[serde(default = "default_status")]
    pub status: String,
    /// HTTP error code that caused exhaustion (0 if ok).
    #[serde(default)]
    pub last_error_code: u16,
    /// Unix timestamp when the credential may be retried.
    #[serde(default)]
    pub reset_at: i64,
    /// Cumulative request count through this credential.
    #[serde(default)]
    pub request_count: u64,
    /// Extra provider-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_status() -> String {
    STATUS_OK.to_string()
}

impl PooledCredential {
    /// Create a new API key credential.
    pub fn new_api_key(provider: &str, key: &str, source: &str, priority: i32) -> Self {
        Self {
            provider: provider.to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            label: format!("{}...{}", &key[..key.len().min(6)], &key[key.len().saturating_sub(4)..]),
            auth_type: "api_key".to_string(),
            priority,
            source: source.to_string(),
            access_token: key.to_string(),
            refresh_token: None,
            base_url: None,
            status: STATUS_OK.to_string(),
            last_error_code: 0,
            reset_at: 0,
            request_count: 0,
            extra: HashMap::new(),
        }
    }

    /// Whether this credential is currently available for use.
    pub fn is_available(&self) -> bool {
        if self.status == STATUS_OK {
            return true;
        }
        // Check cooldown expiration.
        if self.status == STATUS_EXHAUSTED && self.reset_at > 0 {
            let now = now_epoch();
            return now >= self.reset_at;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

/// A pool of credentials for a single provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialPool {
    pub provider: String,
    pub entries: Vec<PooledCredential>,
    pub strategy: SelectionStrategy,
    /// Round-robin cursor (not persisted).
    #[serde(skip)]
    round_robin_index: usize,
}

impl CredentialPool {
    /// Create an empty pool for a provider.
    pub fn new(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            entries: Vec::new(),
            strategy: SelectionStrategy::default(),
            round_robin_index: 0,
        }
    }

    /// Load pool from `~/.openhermes/auth.json`, seeding from env vars.
    pub fn load(provider: &str) -> Result<Self> {
        let path = auth_file_path();
        let mut store = load_auth_store(&path)?;

        let mut pool: CredentialPool = store
            .remove(provider)
            .unwrap_or_else(|| CredentialPool::new(provider));

        pool.provider = provider.to_string();

        // Seed from environment variables.
        pool.seed_from_env();

        Ok(pool)
    }

    /// Persist pool to `~/.openhermes/auth.json`.
    pub fn save(&self) -> Result<()> {
        let path = auth_file_path();
        let mut store = load_auth_store(&path).unwrap_or_default();
        store.insert(self.provider.clone(), self.clone());
        save_auth_store(&path, &store)
    }

    /// Add a credential to the pool.
    pub fn add(&mut self, cred: PooledCredential) {
        // Check for duplicates by access_token.
        if self
            .entries
            .iter()
            .any(|e| e.access_token == cred.access_token)
        {
            debug!("Credential already in pool, skipping");
            return;
        }
        info!(provider = %self.provider, label = %cred.label, "Adding credential to pool");
        self.entries.push(cred);
    }

    /// Remove a credential by ID.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    /// Select the next available credential using the configured strategy.
    pub fn select(&mut self) -> Option<&mut PooledCredential> {
        // Clear expired exhaustions first.
        self.clear_expired();

        let available: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_available())
            .map(|(i, _)| i)
            .collect();

        if available.is_empty() {
            warn!(provider = %self.provider, "All credentials exhausted");
            return None;
        }

        let idx = match self.strategy {
            SelectionStrategy::FillFirst => {
                // Lowest priority (= highest preference) first.
                *available
                    .iter()
                    .min_by_key(|&&i| self.entries[i].priority)
                    .unwrap()
            }
            SelectionStrategy::RoundRobin => {
                self.round_robin_index = (self.round_robin_index + 1) % available.len();
                available[self.round_robin_index]
            }
            SelectionStrategy::Random => {
                let mut rng = rand::thread_rng();
                *available.choose(&mut rng).unwrap()
            }
            SelectionStrategy::LeastUsed => {
                *available
                    .iter()
                    .min_by_key(|&&i| self.entries[i].request_count)
                    .unwrap()
            }
        };

        self.entries[idx].request_count += 1;
        Some(&mut self.entries[idx])
    }

    /// Mark a credential as exhausted (e.g. after a 429 or 402 error).
    pub fn mark_exhausted(&mut self, id: &str, error_code: u16, reset_at: Option<i64>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.status = STATUS_EXHAUSTED.to_string();
            entry.last_error_code = error_code;
            entry.reset_at = reset_at.unwrap_or_else(|| now_epoch() + EXHAUSTED_TTL_SECONDS);
            info!(
                provider = %self.provider,
                label = %entry.label,
                error_code,
                "Credential marked exhausted"
            );
        }
    }

    /// Mark a credential as OK (e.g. after a successful request).
    pub fn mark_ok(&mut self, id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            if entry.status != STATUS_OK {
                debug!(provider = %self.provider, label = %entry.label, "Credential restored to OK");
            }
            entry.status = STATUS_OK.to_string();
            entry.last_error_code = 0;
            entry.reset_at = 0;
        }
    }

    /// Number of available (non-exhausted) credentials.
    pub fn available_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_available()).count()
    }

    /// Total number of credentials in the pool.
    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    // -- Internal helpers ---------------------------------------------------

    /// Seed credentials from environment variables.
    fn seed_from_env(&mut self) {
        for &(prov, env_var) in ENV_CREDENTIAL_MAP {
            if prov != self.provider {
                continue;
            }
            if let Ok(key) = std::env::var(env_var) {
                let key = key.trim().to_string();
                if key.is_empty() {
                    continue;
                }
                // Check if already present.
                if self.entries.iter().any(|e| e.access_token == key) {
                    continue;
                }
                let priority = self.entries.len() as i32;
                let cred = PooledCredential::new_api_key(&self.provider, &key, "env", priority);
                info!(provider = %self.provider, source = "env", "Seeded credential from {}", env_var);
                self.entries.push(cred);
            }
        }
    }

    /// Clear expired exhaustion statuses.
    fn clear_expired(&mut self) {
        let now = now_epoch();
        for entry in &mut self.entries {
            if entry.status == STATUS_EXHAUSTED && entry.reset_at > 0 && now >= entry.reset_at {
                debug!(
                    provider = %self.provider,
                    label = %entry.label,
                    "Credential cooldown expired, restoring to OK"
                );
                entry.status = STATUS_OK.to_string();
                entry.last_error_code = 0;
                entry.reset_at = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// File persistence
// ---------------------------------------------------------------------------

type AuthStore = HashMap<String, CredentialPool>;

fn auth_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".openhermes").join("auth.json")
}

fn load_auth_store(path: &PathBuf) -> Result<AuthStore> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(path).context("Failed to read auth.json")?;
    let store: AuthStore = serde_json::from_str(&data).context("Failed to parse auth.json")?;
    Ok(store)
}

fn save_auth_store(path: &PathBuf, store: &AuthStore) -> Result<()> {
    // Ensure directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create auth directory")?;
    }

    let data = serde_json::to_string_pretty(store).context("Failed to serialize auth store")?;
    fs::write(path, data).context("Failed to write auth.json")?;

    // Set file permissions to 0600 on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_pool() {
        let pool = CredentialPool::new("openai");
        assert_eq!(pool.provider, "openai");
        assert!(pool.entries.is_empty());
    }

    #[test]
    fn test_add_and_select() {
        let mut pool = CredentialPool::new("openai");
        pool.add(PooledCredential::new_api_key("openai", "sk-test-key-123456789", "manual", 0));
        pool.add(PooledCredential::new_api_key("openai", "sk-test-key-987654321", "manual", 1));
        assert_eq!(pool.total_count(), 2);
        assert_eq!(pool.available_count(), 2);

        let cred = pool.select().unwrap();
        assert_eq!(cred.provider, "openai");
        assert_eq!(cred.priority, 0); // fill_first picks lowest priority
    }

    #[test]
    fn test_duplicate_prevention() {
        let mut pool = CredentialPool::new("openai");
        pool.add(PooledCredential::new_api_key("openai", "sk-test-key-123456789", "manual", 0));
        pool.add(PooledCredential::new_api_key("openai", "sk-test-key-123456789", "manual", 1));
        assert_eq!(pool.total_count(), 1); // duplicate rejected
    }

    #[test]
    fn test_exhaustion_and_recovery() {
        let mut pool = CredentialPool::new("openai");
        pool.add(PooledCredential::new_api_key("openai", "sk-test-key-123456789", "manual", 0));
        let id = pool.entries[0].id.clone();

        pool.mark_exhausted(&id, 429, Some(now_epoch() + 3600));
        assert_eq!(pool.available_count(), 0);

        pool.mark_ok(&id);
        assert_eq!(pool.available_count(), 1);
    }

    #[test]
    fn test_least_used_strategy() {
        let mut pool = CredentialPool::new("openai");
        pool.strategy = SelectionStrategy::LeastUsed;

        let mut cred1 = PooledCredential::new_api_key("openai", "sk-key-aaa-123456789", "manual", 0);
        cred1.request_count = 10;
        pool.add(cred1);

        let mut cred2 = PooledCredential::new_api_key("openai", "sk-key-bbb-987654321", "manual", 1);
        cred2.request_count = 2;
        pool.add(cred2);

        let selected = pool.select().unwrap();
        // Should pick the one with fewer requests (was 2, now incremented to 3).
        assert_eq!(selected.request_count, 3);
    }

    #[test]
    fn test_credential_label() {
        let cred = PooledCredential::new_api_key("openai", "sk-abcdefghij1234567890", "env", 0);
        assert!(cred.label.starts_with("sk-abc"));
        assert!(cred.label.ends_with("7890"));
    }

    #[test]
    fn test_selection_strategies() {
        // Just ensure all strategies don't panic.
        for strategy in [
            SelectionStrategy::FillFirst,
            SelectionStrategy::RoundRobin,
            SelectionStrategy::Random,
            SelectionStrategy::LeastUsed,
        ] {
            let mut pool = CredentialPool::new("test");
            pool.strategy = strategy;
            pool.add(PooledCredential::new_api_key("test", "key-123456789012345", "manual", 0));
            assert!(pool.select().is_some());
        }
    }
}
