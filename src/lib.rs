//! Diagnostic engine and command-line application for vLLM.
//!
//! The [`core`] module contains the provider boundary, normalized metrics,
//! rules, assessment, and privacy-limited observation contract. The `cli`
//! module supplies collection, persistence, rendering, and command
//! orchestration.

#[cfg(feature = "cli")]
pub mod cli;
pub mod core;

/// Return the package version embedded at compile time.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
