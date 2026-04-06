//! Iteration budget for controlling agent loop execution.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Thread-safe iteration counter for an agent.
///
/// Each agent (parent or subagent) gets its own `IterationBudget`.
/// The parent's budget is capped at `max_iterations` (default 90).
/// Each subagent gets an independent budget capped at
/// `delegation.max_iterations` (default 50).
pub struct IterationBudget {
    max_total: usize,
    used: AtomicUsize,
}

impl IterationBudget {
    /// Create a new iteration budget
    pub fn new(max_total: usize) -> Self {
        Self {
            max_total,
            used: AtomicUsize::new(0),
        }
    }

    /// Try to consume one iteration. Returns true if allowed.
    pub fn consume(&self) -> bool {
        self.used.fetch_add(1, Ordering::SeqCst) < self.max_total
    }

    /// Give back one iteration (e.g. for execute_code turns).
    pub fn refund(&self) {
        self.used.fetch_sub(1, Ordering::SeqCst);
    }

    /// Number of iterations used
    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    /// Number of iterations remaining
    pub fn remaining(&self) -> usize {
        self.max_total.saturating_sub(self.used.load(Ordering::SeqCst))
    }

    /// Maximum total iterations
    pub fn max_total(&self) -> usize {
        self.max_total
    }
}
