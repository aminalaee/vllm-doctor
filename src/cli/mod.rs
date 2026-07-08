//! CLI argument definitions.
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Diagnose a vLLM /metrics or Prometheus endpoint
    Diagnose {
        /// vLLM /metrics or Prometheus URL (e.g. http://host:8000/metrics)
        url: String,
        /// Time window, e.g. 1h, 30m, or now (now means last 5m)
        #[arg(short, long, default_value = "now")]
        since: String,
        /// Filter metrics by model_name label
        #[arg(short, long)]
        model: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        output: Format,
        /// Show additional diagnostic detail
        #[arg(short, long)]
        verbose: bool,
        /// Persist this diagnosis run to the local database
        #[arg(long)]
        save: bool,
        /// Refresh continuously every 5s
        #[arg(short, long)]
        watch: bool,
        /// HTTP request timeout in seconds
        #[arg(short = 't', long, default_value_t = 10.0)]
        timeout: f64,
        /// Path to config file (default: vllm-doctor.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Local diagnosis history commands
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Run database migrations
    Migrate {
        /// Path to config file (default: vllm-doctor.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// List saved diagnosis runs
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        output: Format,
        /// Show additional columns
        #[arg(short, long)]
        verbose: bool,
        /// Path to config file (default: vllm-doctor.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Show a saved diagnosis run
    Show {
        /// ID of the saved diagnosis run
        run_id: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        output: Format,
        /// Show additional diagnostic detail
        #[arg(short, long)]
        verbose: bool,
        /// Path to config file (default: vllm-doctor.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_migrate_command() {
        let args = Args::parse_from(["vllm-doctor", "migrate"]);
        assert!(matches!(args.command, Command::Migrate { .. }));
    }

    #[test]
    fn parse_diagnose_command() {
        let args = Args::parse_from([
            "vllm-doctor",
            "diagnose",
            "http://localhost:8000/metrics",
            "--since",
            "1h",
            "--model",
            "llama",
            "--output",
            "json",
            "--verbose",
            "--save",
            "--timeout",
            "30",
        ]);
        match args.command {
            Command::Diagnose {
                url,
                since,
                model,
                output,
                verbose,
                save,
                watch,
                timeout,
                ..
            } => {
                assert_eq!(url, "http://localhost:8000/metrics");
                assert_eq!(since, "1h");
                assert_eq!(model.as_deref(), Some("llama"));
                assert_eq!(output, Format::Json);
                assert!(verbose);
                assert!(save);
                assert!(!watch);
                assert_eq!(timeout, 30.0);
            }
            _ => panic!("expected diagnose command"),
        }
    }

    #[test]
    fn parse_diagnose_defaults() {
        let args = Args::parse_from(["vllm-doctor", "diagnose", "http://host/metrics"]);
        match args.command {
            Command::Diagnose {
                since,
                output,
                verbose,
                model,
                save,
                watch,
                timeout,
                config,
                ..
            } => {
                assert_eq!(since, "now");
                assert_eq!(output, Format::Text);
                assert!(!verbose);
                assert_eq!(model, None);
                assert!(!save);
                assert!(!watch);
                assert_eq!(timeout, 10.0);
                assert_eq!(config, None);
            }
            _ => panic!("expected diagnose command"),
        }
    }

    #[test]
    fn diagnose_requires_url() {
        assert!(Args::try_parse_from(["vllm-doctor", "diagnose"]).is_err());
    }

    #[test]
    fn parse_history_list() {
        let args = Args::parse_from(["vllm-doctor", "history", "list"]);
        match args.command {
            Command::History {
                command:
                    HistoryCommand::List {
                        output, verbose, ..
                    },
            } => {
                assert_eq!(output, Format::Text);
                assert!(!verbose);
            }
            _ => panic!("expected history list"),
        }
    }

    #[test]
    fn parse_history_show() {
        let args = Args::parse_from(["vllm-doctor", "history", "show", "abc-123"]);
        match args.command {
            Command::History {
                command: HistoryCommand::Show { run_id, .. },
            } => {
                assert_eq!(run_id, "abc-123");
            }
            _ => panic!("expected history show"),
        }
    }
}
