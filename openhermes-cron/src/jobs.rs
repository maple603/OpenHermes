//! Cron job storage and management.
//!
//! Stores jobs in `~/.openhermes/cron/jobs.json` with atomic writes.
//! Supports cron expressions, intervals, one-shot, and ISO timestamps.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// A scheduled cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: ScheduleKind,
    pub model: Option<String>,
    pub delivery: Option<String>,
    pub status: JobStatus,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub max_runs: Option<u64>,
    pub created_at: DateTime<Utc>,
}

/// Schedule types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ScheduleKind {
    /// Run once at a specific time.
    Once(DateTime<Utc>),
    /// Run at a fixed interval.
    Interval(u64), // seconds
    /// Standard cron expression (minute hour day month weekday).
    Cron(String),
}

/// Job status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Active,
    Paused,
    Completed,
    Failed,
}

/// Persistent job store backed by a JSON file.
pub struct JobStore {
    path: PathBuf,
}

impl JobStore {
    /// Create a new store at the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Default store location: `~/.openhermes/cron/jobs.json`.
    pub fn default_path() -> PathBuf {
        openhermes_constants::get_hermes_home()
            .join("cron")
            .join("jobs.json")
    }

    /// Load all jobs from disk.
    pub fn load_jobs(&self) -> Result<Vec<CronJob>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read jobs from {}", self.path.display()))?;
        let jobs: Vec<CronJob> = serde_json::from_str(&data)
            .with_context(|| "Failed to parse jobs JSON")?;
        Ok(jobs)
    }

    /// Save all jobs to disk (atomic write).
    pub fn save_jobs(&self, jobs: &[CronJob]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = serde_json::to_string_pretty(jobs)?;
        let tmp_path = self.path.with_extension("tmp");
        std::fs::write(&tmp_path, &data)?;
        std::fs::rename(&tmp_path, &self.path)?;

        debug!(count = jobs.len(), "Jobs saved to {}", self.path.display());
        Ok(())
    }

    /// Create a new job.
    pub fn create_job(
        &self,
        name: &str,
        prompt: &str,
        schedule_expr: &str,
        model: Option<&str>,
        delivery: Option<&str>,
        max_runs: Option<u64>,
    ) -> Result<CronJob> {
        let schedule = parse_schedule(schedule_expr)?;
        let next_run = compute_next_run(&schedule, None);

        let job = CronJob {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            name: name.to_string(),
            prompt: prompt.to_string(),
            schedule,
            model: model.map(String::from),
            delivery: delivery.map(String::from),
            status: JobStatus::Active,
            next_run_at: next_run,
            last_run_at: None,
            run_count: 0,
            max_runs,
            created_at: Utc::now(),
        };

        let mut jobs = self.load_jobs()?;
        jobs.push(job.clone());
        self.save_jobs(&jobs)?;

        info!(id = %job.id, name = name, "Created cron job");
        Ok(job)
    }

    /// List all jobs.
    pub fn list_jobs(&self) -> Result<Vec<CronJob>> {
        self.load_jobs()
    }

    /// Get a job by ID.
    pub fn get_job(&self, id: &str) -> Result<Option<CronJob>> {
        let jobs = self.load_jobs()?;
        Ok(jobs.into_iter().find(|j| j.id == id))
    }

    /// Update a job (replace by ID).
    pub fn update_job(&self, job: &CronJob) -> Result<()> {
        let mut jobs = self.load_jobs()?;
        if let Some(pos) = jobs.iter().position(|j| j.id == job.id) {
            jobs[pos] = job.clone();
            self.save_jobs(&jobs)?;
            Ok(())
        } else {
            anyhow::bail!("Job not found: {}", job.id)
        }
    }

    /// Pause a job.
    pub fn pause_job(&self, id: &str) -> Result<()> {
        let mut jobs = self.load_jobs()?;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Paused;
            self.save_jobs(&jobs)?;
            info!(id = id, "Job paused");
            Ok(())
        } else {
            anyhow::bail!("Job not found: {}", id)
        }
    }

    /// Resume a paused job.
    pub fn resume_job(&self, id: &str) -> Result<()> {
        let mut jobs = self.load_jobs()?;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Active;
            job.next_run_at = compute_next_run(&job.schedule, job.last_run_at.as_ref());
            self.save_jobs(&jobs)?;
            info!(id = id, "Job resumed");
            Ok(())
        } else {
            anyhow::bail!("Job not found: {}", id)
        }
    }

    /// Remove a job.
    pub fn remove_job(&self, id: &str) -> Result<()> {
        let mut jobs = self.load_jobs()?;
        let before = jobs.len();
        jobs.retain(|j| j.id != id);
        if jobs.len() == before {
            anyhow::bail!("Job not found: {}", id)
        }
        self.save_jobs(&jobs)?;
        info!(id = id, "Job removed");
        Ok(())
    }

    /// Get jobs that are due to run.
    pub fn get_due_jobs(&self) -> Result<Vec<CronJob>> {
        let jobs = self.load_jobs()?;
        let now = Utc::now();
        Ok(jobs
            .into_iter()
            .filter(|j| {
                j.status == JobStatus::Active
                    && j.next_run_at.map_or(false, |t| t <= now)
            })
            .collect())
    }

    /// Mark a job as executed and compute next run.
    pub fn mark_executed(&self, id: &str, success: bool) -> Result<()> {
        let mut jobs = self.load_jobs()?;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.last_run_at = Some(Utc::now());
            job.run_count += 1;

            if !success {
                job.status = JobStatus::Failed;
            } else if let Some(max) = job.max_runs {
                if job.run_count >= max {
                    job.status = JobStatus::Completed;
                    job.next_run_at = None;
                } else {
                    job.next_run_at = compute_next_run(&job.schedule, job.last_run_at.as_ref());
                }
            } else {
                job.next_run_at = compute_next_run(&job.schedule, job.last_run_at.as_ref());
            }

            // One-shot jobs complete after first run
            if matches!(job.schedule, ScheduleKind::Once(_)) {
                job.status = JobStatus::Completed;
                job.next_run_at = None;
            }

            self.save_jobs(&jobs)?;
            Ok(())
        } else {
            anyhow::bail!("Job not found: {}", id)
        }
    }
}

/// Parse a schedule expression into a ScheduleKind.
///
/// Supports:
/// - Cron expressions (5 fields): "0 * * * *"
/// - Intervals: "30m", "2h", "1d"
/// - ISO timestamps: "2025-01-01T00:00:00Z"
/// - Keywords: "once" (run once at next tick)
pub fn parse_schedule(expr: &str) -> Result<ScheduleKind> {
    let expr = expr.trim();

    // "once" keyword
    if expr.eq_ignore_ascii_case("once") {
        return Ok(ScheduleKind::Once(Utc::now()));
    }

    // Interval: number + unit suffix
    if let Some(interval) = parse_interval(expr) {
        return Ok(ScheduleKind::Interval(interval));
    }

    // ISO timestamp
    if let Ok(dt) = expr.parse::<DateTime<Utc>>() {
        return Ok(ScheduleKind::Once(dt));
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(expr) {
        return Ok(ScheduleKind::Once(dt.with_timezone(&Utc)));
    }

    // Cron expression (5 fields)
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        // Basic validation: each field should contain valid cron chars
        let valid = parts.iter().all(|p| {
            p.chars().all(|c| c.is_ascii_digit() || "*,/-".contains(c))
        });
        if valid {
            return Ok(ScheduleKind::Cron(expr.to_string()));
        }
    }

    anyhow::bail!(
        "Invalid schedule expression: '{}'. Use cron (5 fields), interval (30m, 2h, 1d), ISO timestamp, or 'once'.",
        expr
    )
}

/// Parse an interval string like "30m", "2h", "1d" into seconds.
fn parse_interval(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.len() < 2 {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(num),
        "m" => Some(num * 60),
        "h" => Some(num * 3600),
        "d" => Some(num * 86400),
        "w" => Some(num * 604800),
        _ => None,
    }
}

/// Compute the next run time for a schedule.
fn compute_next_run(
    schedule: &ScheduleKind,
    _last_run: Option<&DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match schedule {
        ScheduleKind::Once(dt) => Some(*dt),
        ScheduleKind::Interval(secs) => {
            Some(Utc::now() + Duration::seconds(*secs as i64))
        }
        ScheduleKind::Cron(_expr) => {
            // For now, approximate: next minute boundary
            // Full cron parsing would use the `croner` crate
            Some(Utc::now() + Duration::minutes(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("30m"), Some(1800));
        assert_eq!(parse_interval("2h"), Some(7200));
        assert_eq!(parse_interval("1d"), Some(86400));
        assert_eq!(parse_interval("10s"), Some(10));
        assert_eq!(parse_interval("invalid"), None);
    }

    #[test]
    fn test_parse_schedule_once() {
        let s = parse_schedule("once").unwrap();
        assert!(matches!(s, ScheduleKind::Once(_)));
    }

    #[test]
    fn test_parse_schedule_interval() {
        let s = parse_schedule("30m").unwrap();
        assert!(matches!(s, ScheduleKind::Interval(1800)));
    }

    #[test]
    fn test_parse_schedule_cron() {
        let s = parse_schedule("0 * * * *").unwrap();
        assert!(matches!(s, ScheduleKind::Cron(_)));
    }

    #[test]
    fn test_parse_schedule_invalid() {
        assert!(parse_schedule("xyz garbage").is_err());
    }

    #[test]
    fn test_job_store_roundtrip() {
        let tmp = std::env::temp_dir().join("openhermes_test_jobs.json");
        let _ = std::fs::remove_file(&tmp);
        let store = JobStore::new(tmp.clone());

        let job = store.create_job(
            "test", "run tests", "30m", None, None, Some(5),
        ).unwrap();
        assert_eq!(job.name, "test");

        let jobs = store.list_jobs().unwrap();
        assert_eq!(jobs.len(), 1);

        store.pause_job(&job.id).unwrap();
        let paused = store.get_job(&job.id).unwrap().unwrap();
        assert_eq!(paused.status, JobStatus::Paused);

        store.remove_job(&job.id).unwrap();
        let empty = store.list_jobs().unwrap();
        assert!(empty.is_empty());

        let _ = std::fs::remove_file(&tmp);
    }
}
