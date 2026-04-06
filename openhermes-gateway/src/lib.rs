//! Messaging platform gateway for OpenHermes Agent.
//!
//! Placeholder implementation - will support 17+ platforms.

/// Gateway runner
pub struct GatewayRunner;

impl GatewayRunner {
    pub async fn start(&self) -> anyhow::Result<()> {
        // TODO: Implement gateway with platform adapters
        tracing::info!("Gateway not yet implemented");
        Ok(())
    }
}

impl Default for GatewayRunner {
    fn default() -> Self {
        Self
    }
}
