//! CLI layer: argument parsing, HTTP clients, persistence, rendering, and the
//! diagnosis runner. These modules depend on reqwest, sqlx, clap, comfy-table,
//! owo-colors, dirs, and figment. They build on top of [`core`](crate::core).
pub mod args;
pub mod clients;
pub mod collector;
pub mod commands;
pub mod config;
pub mod observability;
pub mod probes;
pub mod providers;
pub mod reports;
pub mod runner;
pub mod stores;
pub mod upload;
