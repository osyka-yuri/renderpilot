//! Stable feature labels for durable game-file transactions.
//!
//! Re-exported from domain so orchestration callers keep a short path while
//! storage and other crates can import the same persisted contract.

pub use renderpilot_domain::mutation_features::*;
