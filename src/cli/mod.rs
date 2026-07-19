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

fn positive_seconds(s: &str) -> Result<f64, String> {
    let value: f64 = s.parse().map_err(|_| format!("`{s}` is not a number"))?;
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err("must be a positive number of seconds".to_string())
    }
}

/// Parse and validate a `NAME:VALUE` header. The value is never echoed in
/// errors (it may hold a token); messages reference the header name only.
fn parse_header(s: &str) -> Result<(String, String), String> {
    let (name, value) = s.split_once(':').ok_or("expected NAME:VALUE")?;
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() {
        return Err("header name is empty".to_string());
    }
    reqwest::header::HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| format!("invalid header name `{name}`"))?;
    reqwest::header::HeaderValue::from_str(value)
        .map_err(|_| format!("invalid value for header `{name}`"))?;
    Ok((name.to_string(), value.to_string()))
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
        /// Refresh continuously (interval set by --interval)
        #[arg(short, long)]
        watch: bool,
        /// Base seconds between refreshes in --watch mode (jittered by ±20%)
        #[arg(short = 'i', long, default_value_t = 5.0, value_parser = positive_seconds)]
        interval: f64,
        /// HTTP request timeout in seconds
        #[arg(short = 't', long, default_value_t = 10.0, value_parser = positive_seconds)]
        timeout: f64,
        /// Extra HTTP header to send with every request (NAME:VALUE, repeatable)
        #[arg(long = "header", value_parser = parse_header, value_name = "NAME:VALUE")]
        headers: Vec<(String, String)>,
        /// Path to a PEM file containing a CA certificate to trust
        #[arg(long, value_name = "PATH")]
        ca_cert: Option<PathBuf>,
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
            "--interval",
            "2",
            "--timeout",
            "30",
            "--header",
            "Authorization: Bearer secret",
            "--ca-cert",
            "/path/to/ca.pem",
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
                interval,
                timeout,
                headers,
                ca_cert,
                ..
            } => {
                assert_eq!(url, "http://localhost:8000/metrics");
                assert_eq!(since, "1h");
                assert_eq!(model.as_deref(), Some("llama"));
                assert_eq!(output, Format::Json);
                assert!(verbose);
                assert!(save);
                assert!(!watch);
                assert_eq!(interval, 2.0);
                assert_eq!(timeout, 30.0);
                assert_eq!(
                    headers,
                    vec![("Authorization".to_string(), "Bearer secret".to_string())]
                );
                assert_eq!(
                    ca_cert.as_deref(),
                    Some(std::path::Path::new("/path/to/ca.pem"))
                );
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
                interval,
                timeout,
                config,
                headers,
                ca_cert,
                ..
            } => {
                assert_eq!(since, "now");
                assert_eq!(output, Format::Text);
                assert!(!verbose);
                assert_eq!(model, None);
                assert!(!save);
                assert!(!watch);
                assert_eq!(interval, 5.0);
                assert_eq!(timeout, 10.0);
                assert_eq!(config, None);
                assert!(headers.is_empty());
                assert_eq!(ca_cert, None);
            }
            _ => panic!("expected diagnose command"),
        }
    }

    #[test]
    fn diagnose_requires_url() {
        assert!(Args::try_parse_from(["vllm-doctor", "diagnose"]).is_err());
    }

    #[test]
    fn diagnose_rejects_malformed_header() {
        for bad in ["no-colon", ":emptyname"] {
            assert!(
                Args::try_parse_from([
                    "vllm-doctor",
                    "diagnose",
                    "http://host/metrics",
                    "--header",
                    bad
                ])
                .is_err(),
                "`{bad}` should be rejected"
            );
        }
    }

    #[test]
    fn diagnose_rejects_non_positive_seconds() {
        for arg in ["--interval", "--timeout"] {
            for value in ["0", "-1", "nan"] {
                assert!(
                    Args::try_parse_from([
                        "vllm-doctor",
                        "diagnose",
                        "http://host/metrics",
                        arg,
                        value
                    ])
                    .is_err(),
                    "{arg} {value} should be rejected"
                );
            }
        }
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
