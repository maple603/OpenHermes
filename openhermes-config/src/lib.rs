//! Configuration management for OpenHermes Agent.
//!
//! Handles loading and validation of config.yaml and .env files.

mod config;
mod env;
mod types;

pub use config::{load_config, save_config, DEFAULT_CONFIG};
pub use env::load_dotenv;
pub use types::*;
