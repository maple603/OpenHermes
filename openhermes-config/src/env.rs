//! Environment variable loading from .env files.

use anyhow::{Context, Result};
use tracing::info;

/// Load environment variables from ~/.hermes/.env
pub fn load_dotenv() -> Result<()> {
    let env_path = openhermes_constants::get_hermes_home().join(".env");

    if !env_path.exists() {
        info!(
            "No .env file found at {}, using system environment",
            openhermes_constants::display_hermes_home()
        );
        return Ok(());
    }

    info!(
        "Loading environment variables from {}",
        env_path.display()
    );

    dotenvy::from_path(&env_path)
        .with_context(|| format!("Failed to load .env file: {}", env_path.display()))?;

    info!("Environment variables loaded successfully");
    Ok(())
}

/// Load environment variables from a custom path
pub fn load_dotenv_from_path(path: &std::path::PathBuf) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    dotenvy::from_path(path)
        .with_context(|| format!("Failed to load .env file: {}", path.display()))?;

    Ok(())
}

/// Load environment variables from project root .env (for development)
pub fn load_project_dotenv() -> Result<()> {
    let project_env = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(".env");

    if project_env.exists() {
        dotenvy::from_path(&project_env).ok();
        info!("Loaded project .env from {}", project_env.display());
    }

    Ok(())
}
