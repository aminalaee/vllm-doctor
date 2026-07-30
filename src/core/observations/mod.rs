//! Versioned, destination-independent observation contract.
//!
//! The `observations` module owns the typed wire schema that an eventual cloud
//! backend will ingest. It is deliberately separate from `models.rs` so the
//! versioned contract has an explicit boundary and cannot accidentally leak
//! internal diagnosis types.

pub mod v1;
mod window;

pub use window::{WindowParseError, parse_window_seconds};
