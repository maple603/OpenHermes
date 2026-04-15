//! Cron scheduler for OpenHermes Agent.
//!
//! Provides job storage, scheduling, and tick-based execution.

pub mod jobs;
pub mod scheduler;

pub use jobs::{CronJob, JobStatus, JobStore, ScheduleKind};
pub use scheduler::Scheduler;

/// Legacy alias for backward compatibility.
pub type CronScheduler = Scheduler;
