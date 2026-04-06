//! Core Agent loop for OpenHermes.
//!
//! This module provides the main AI agent conversation loop with tool calling capabilities.

mod agent;
mod budget;
#[allow(dead_code)]
mod context_compressor;
mod prompt_builder;
mod types;

pub use agent::AIAgent;
pub use budget::IterationBudget;
pub use types::*;
