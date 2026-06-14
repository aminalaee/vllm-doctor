pub mod cli;
pub mod clients;
pub mod config;
pub mod metrics;
pub mod models;

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
