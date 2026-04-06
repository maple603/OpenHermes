//! Cron scheduler for OpenHermes Agent.
//!
//! Placeholder implementation - will use tokio-cron-scheduler.

/// Cron scheduler
pub struct CronScheduler;

impl CronScheduler {
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        // TODO: Implement cron scheduler
        tracing::info!("Cron scheduler not yet implemented");
        Ok(())
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}
