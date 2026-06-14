//! CLI argument definitions.
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "vllm-doctor",
    version,
    about = "Diagnostic tool for vLLM inference servers"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Diagnose a vLLM /metrics or Prometheus endpoint
    Diagnose,
    /// Local diagnosis history commands
    History,
    /// Run database migrations
    Migrate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_migrate_command() {
        let args = Args::parse_from(["vllm-doctor", "migrate"]);
        assert!(matches!(args.command, Command::Migrate));
    }

    #[test]
    fn parse_diagnose_command() {
        let args = Args::parse_from(["vllm-doctor", "diagnose"]);
        assert!(matches!(args.command, Command::Diagnose));
    }
}
