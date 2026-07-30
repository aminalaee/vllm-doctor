//! Diagnostic core: types and logic shared with the backend.
//!
//! This module tree has no dependency on reqwest, sqlx, clap, comfy-table,
//! owo-colors, dirs, or figment. The backend imports `vllm_doctor::core::*`;
//! the CLI imports both `core` and [`cli`](crate::cli).
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
