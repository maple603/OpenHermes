//! Core Agent loop for OpenHermes.
//!
//! This module provides the main AI agent conversation loop with tool calling capabilities,
//! credential management, usage tracking, error classification, and smart routing.

mod agent;
mod budget;
#[allow(dead_code)]
mod context_compressor;
mod prompt_builder;
mod types;

// P0 Agent Core Features
pub mod redact;
pub mod rate_limit_tracker;
pub mod error_classifier;
pub mod smart_routing;
pub mod title_generator;
pub mod usage_pricing;
pub mod credential_pool;

pub use agent::AIAgent;
pub use budget::IterationBudget;
pub use types::*;
