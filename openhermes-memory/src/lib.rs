//! Memory system for OpenHermes Agent.
//!
//! Placeholder implementation - will be expanded with SQLite + FTS5.

mod memory_manager;
mod builtin_provider;

pub use memory_manager::MemoryManager;
pub use builtin_provider::BuiltinMemoryProvider;
