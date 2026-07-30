//! Diagnostic types and logic separated from CLI-specific I/O.
//!
//! This module tree has no dependency on reqwest, sqlx, clap, comfy-table,
//! owo-colors, dirs, or figment.
pub mod assessment;
pub mod config;
pub mod diagnosis;
pub mod metrics;
pub mod models;
pub mod observations;

pub mod probes;
pub mod providers;
pub mod rules;
pub mod signals;
