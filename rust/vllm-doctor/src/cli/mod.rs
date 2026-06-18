//! CLI argument definitions.
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
        /// Time window, e.g. 1h, 30m, now
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
    },
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
        ]);
        match args.command {
            Command::Diagnose {
                url,
                since,
                model,
                output,
                verbose,
            } => {
                assert_eq!(url, "http://localhost:8000/metrics");
                assert_eq!(since, "1h");
                assert_eq!(model.as_deref(), Some("llama"));
                assert_eq!(output, Format::Json);
                assert!(verbose);
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
                ..
            } => {
                assert_eq!(since, "now");
                assert_eq!(output, Format::Text);
                assert!(!verbose);
                assert_eq!(model, None);
            }
            _ => panic!("expected diagnose command"),
        }
    }

    #[test]
    fn diagnose_requires_url() {
        assert!(Args::try_parse_from(["vllm-doctor", "diagnose"]).is_err());
    }

    #[test]
    fn parse_history_command() {
        let args = Args::parse_from(["vllm-doctor", "history"]);
        assert!(matches!(args.command, Command::History));
    }
}
