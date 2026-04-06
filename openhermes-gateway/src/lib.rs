//! Messaging platform gateway for OpenHermes Agent.

pub mod platform;
pub mod telegram;
pub mod discord;
pub mod session;
pub mod router;

pub use platform::PlatformAdapter;
pub use session::SessionManager;
pub use router::MessageRouter;
