pub mod cli;
pub mod clients;
pub mod collector;
pub mod config;
pub mod diagnosis;
pub mod metrics;
pub mod models;
pub mod probes;
pub mod providers;
pub mod reports;
pub mod rules;
pub mod signals;
pub mod stores;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo() {
        assert_eq!(version(), "0.6.0");
    }
}
