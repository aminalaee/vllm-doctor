use std::io::IsTerminal;

use clap::Parser;

use vllm_doctor::cli::{Args, Command, Format};
use vllm_doctor::config::Config;
use vllm_doctor::diagnosis::diagnose;
use vllm_doctor::providers::resolve_provider;
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::rules::build_registry;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    match args.command {
        Command::Diagnose {
            url,
            since,
            model,
            output,
            verbose,
        } => {
            if let Err(err) = run_diagnose(&url, &since, model.as_deref(), output, verbose).await {
                eprintln!("Error: could not read metrics from {url}: {err}");
                std::process::exit(1);
            }
        }
        Command::History | Command::Migrate => {
            eprintln!("not implemented yet");
            std::process::exit(1);
        }
    }
}

async fn run_diagnose(
    url: &str,
    since: &str,
    model: Option<&str>,
    output: Format,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    let registry = build_registry(&config);
    let provider = resolve_provider(url, 10.0, since, model).await?;
    let result = diagnose(provider.as_ref(), &registry, since, model).await?;
    let report = Report::new(result);
    match output {
        Format::Text => {
            // Color and full width when stdout is a terminal; plain and bounded
            // when piped or redirected so files and pipelines stay clean.
            let stdout = std::io::stdout();
            let opts = RenderOptions {
                verbose,
                width: terminal_size::terminal_size()
                    .map(|(w, _)| w.0 as usize)
                    .unwrap_or(80),
                color: stdout.is_terminal(),
            };
            print!("{}", text::render(&report, &opts));
        }
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json::render(&report, verbose))?
        ),
    }
    Ok(())
}
