//! Session manager for tracking conversations across platforms.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// User session information
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID
    pub session_id: String,
    /// Platform name
    pub platform: String,
    /// User ID
    pub user_id: String,
    /// Chat ID
    pub chat_id: String,
    /// Message count
    pub message_count: usize,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

impl Session {
    /// Create new session
    pub fn new(session_id: String, platform: String, user_id: String, chat_id: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id,
            platform,
            user_id,
            chat_id,
            message_count: 0,
            created_at: now,
            last_activity: now,
        }
    }

    /// Increment message count
    pub fn increment_messages(&mut self) {
        self.message_count += 1;
        self.last_activity = chrono::Utc::now();
    }
}

/// Session manager
pub struct SessionManager {
    /// Active sessions (user_id -> Session)
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create session for user
    pub async fn get_or_create_session(
        &self,
        user_id: &str,
        platform: &str,
        chat_id: &str,
    ) -> Session {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(user_id) {
            session.increment_messages();
            session.clone()
        } else {
            let session_id = format!("session_{}", user_id);
            let session = Session::new(
                session_id,
                platform.to_string(),
                user_id.to_string(),
                chat_id.to_string(),
            );
            
            sessions.insert(user_id.to_string(), session.clone());
            info!(user_id = user_id, platform = platform, "New session created");
            session
        }
    }

    /// Get session for user
    pub async fn get_session(&self, user_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(user_id).cloned()
    }

    /// Remove session
    pub async fn remove_session(&self, user_id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.remove(user_id);
        
        if session.is_some() {
            info!(user_id = user_id, "Session removed");
        }
        
        session
    }

    /// Get active session count
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// Get all active sessions
    pub async fn get_all_sessions(&self) -> Vec<Session> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Clean up inactive sessions (older than max_age_minutes)
    pub async fn cleanup_inactive(&self, max_age_minutes: u64) {
        let mut sessions = self.sessions.write().await;
        let now = chrono::Utc::now();
        let max_age = chrono::Duration::minutes(max_age_minutes as i64);
        
        let initial_count = sessions.len();
        
        sessions.retain(|user_id, session| {
            let is_active = (now - session.last_activity) < max_age;
            if !is_active {
                info!(user_id = user_id, "Inactive session cleaned up");
            }
            is_active
        });
        
        let cleaned = initial_count - sessions.len();
        if cleaned > 0 {
            info!(cleaned = cleaned, "Inactive sessions cleaned up");
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
