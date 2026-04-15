//! Tick-based cron scheduler.
//!
//! Runs a background loop that checks for due jobs every 60 seconds.
//! Uses file-based locking for cross-process safety.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::jobs::{JobStore};

/// Default tick interval (seconds).
const TICK_INTERVAL_SECS: u64 = 60;

/// Default inactivity timeout for job execution (seconds).
const DEFAULT_JOB_TIMEOUT_SECS: u64 = 600;

/// Cron scheduler that runs a tick loop.
pub struct Scheduler {
    store: JobStore,
    tick_interval: Duration,
    job_timeout: Duration,
    output_dir: PathBuf,
}

impl Scheduler {
    /// Create a new scheduler with default settings.
    pub fn new() -> Self {
        let store = JobStore::new(JobStore::default_path());
        let output_dir = openhermes_constants::get_hermes_home()
            .join("cron")
            .join("output");
        let timeout = std::env::var("HERMES_CRON_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_JOB_TIMEOUT_SECS);

        Self {
            store,
            tick_interval: Duration::from_secs(TICK_INTERVAL_SECS),
            job_timeout: Duration::from_secs(timeout),
            output_dir,
        }
    }

    /// Start the scheduler loop (runs until cancelled).
    pub async fn start(&self) -> Result<()> {
        info!(
            interval_secs = self.tick_interval.as_secs(),
            timeout_secs = self.job_timeout.as_secs(),
            "Cron scheduler started"
        );

        let mut ticker = interval(self.tick_interval);
        loop {
            ticker.tick().await;
            if let Err(e) = self.tick().await {
                warn!("Scheduler tick failed: {}", e);
            }
        }
    }

    /// Single tick: check for due jobs and execute them.
    pub async fn tick(&self) -> Result<()> {
        // Try to acquire file lock
        let lock_path = self.store_dir().join(".tick.lock");
        if lock_path.exists() {
            // Check if lock is stale (> 5 minutes old)
            if let Ok(meta) = std::fs::metadata(&lock_path) {
                if let Ok(modified) = meta.modified() {
                    let age = std::time::SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default();
                    if age < Duration::from_secs(300) {
                        debug!("Another tick is running, skipping");
                        return Ok(());
                    }
                }
            }
        }

        // Create lock file
        let _ = std::fs::create_dir_all(lock_path.parent().unwrap_or(&self.store_dir()));
        std::fs::write(&lock_path, Utc::now().to_rfc3339())?;

        let result = self.execute_due_jobs().await;

        // Release lock
        let _ = std::fs::remove_file(&lock_path);

        result
    }

    /// Execute all due jobs.
    async fn execute_due_jobs(&self) -> Result<()> {
        let due_jobs = self.store.get_due_jobs()?;
        if due_jobs.is_empty() {
            return Ok(());
        }

        info!(count = due_jobs.len(), "Found due cron jobs");

        for job in &due_jobs {
            info!(id = %job.id, name = %job.name, "Executing cron job");

            let result = tokio::time::timeout(
                self.job_timeout,
                execute_job_prompt(&job.prompt, job.model.as_deref()),
            )
            .await;

            let (success, output) = match result {
                Ok(Ok(output)) => (true, output),
                Ok(Err(e)) => {
                    warn!(id = %job.id, error = %e, "Job execution failed");
                    (false, format!("Error: {}", e))
                }
                Err(_) => {
                    warn!(id = %job.id, "Job timed out");
                    (false, "Job execution timed out".to_string())
                }
            };

            // Save output
            if let Err(e) = self.save_output(&job.id, &output) {
                warn!(id = %job.id, error = %e, "Failed to save job output");
            }

            // Update job state
            if let Err(e) = self.store.mark_executed(&job.id, success) {
                warn!(id = %job.id, error = %e, "Failed to update job state");
            }
        }

        Ok(())
    }

    /// Save job output to file.
    fn save_output(&self, job_id: &str, output: &str) -> Result<()> {
        let output_dir = self.output_dir.join(job_id);
        std::fs::create_dir_all(&output_dir)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let file_path = output_dir.join(format!("{}.md", timestamp));
        std::fs::write(&file_path, output)?;

        debug!(path = %file_path.display(), "Saved job output");
        Ok(())
    }

    /// Get the store directory.
    fn store_dir(&self) -> PathBuf {
        openhermes_constants::get_hermes_home().join("cron")
    }

    /// Get a reference to the job store.
    pub fn store(&self) -> &JobStore {
        &self.store
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a job prompt using a simple LLM call.
///
/// Uses the same provider resolution as openhermes-tools/llm_client
/// to avoid circular dependencies with openhermes-core.
async fn execute_job_prompt(prompt: &str, _model: Option<&str>) -> Result<String> {
    // Resolve available providers
    let _providers: Vec<(&str, String, String)> = [
        ("OPENROUTER_API_KEY", "https://openrouter.ai/api/v1/chat/completions", "google/gemini-2.5-flash"),
        ("OPENAI_API_KEY", 
         &std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()) ,
         "gpt-4.1-mini"),
    ].iter().filter_map(|(key, url, model)| {
        std::env::var(key).ok().map(|k| (key.to_owned(), format!("{}/chat/completions", url), model.to_string(), k))
    }).map(|(_, url, _model, key)| ("bearer", url, key)).collect::<Vec<_>>();

    // Simplified: just try the first available provider
    let (api_key_env, base_url, model) = if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        (key, "https://openrouter.ai/api/v1/chat/completions".to_string(), "google/gemini-2.5-flash")
    } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        let base = std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        (key, format!("{}/chat/completions", base), "gpt-4.1-mini")
    } else {
        anyhow::bail!("No LLM provider configured for cron execution");
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&base_url)
        .header("Authorization", format!("Bearer {}", api_key_env))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": format!("You are executing a scheduled cron job. Complete the following task:\n\n{}", prompt)}],
            "max_tokens": 4096,
        }))
        .send()
        .await?;

    let data: serde_json::Value = resp.json().await?;
    Ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
}
