use std::io::IsTerminal;

use clap::Parser;
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_BORDERS_ONLY};

use vllm_doctor::cli::{Args, Command, Format, HistoryCommand};
use vllm_doctor::config::{Config, load_config};
use vllm_doctor::diagnosis::diagnose;
use vllm_doctor::models::{DiagnosisResult, Health};
use vllm_doctor::providers::resolve_provider;
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::rules::build_registry;
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err(code) = run(args).await {
        std::process::exit(code);
    }
}

async fn run(args: Args) -> Result<(), i32> {
    match args.command {
        Command::Diagnose {
            url,
            since,
            model,
            output,
            verbose,
            save,
            watch,
            config,
        } => {
            let cfg = load_config_or_default(config.as_deref());
            let result = if watch {
                run_watch(&url, &since, model.as_deref(), output, verbose, save, &cfg).await
            } else {
                run_diagnose(&url, &since, model.as_deref(), output, verbose, save, &cfg).await
            };
            match result {
                Ok(()) => {}
                Err(DiagnoseError::Fetch(e)) => {
                    eprintln!("Error: could not read metrics from {url}: {e}");
                    return Err(1);
                }
                Err(DiagnoseError::Save(e)) => {
                    eprintln!("Error: failed to save run: {e}");
                    return Err(1);
                }
                Err(DiagnoseError::Render(e)) => {
                    eprintln!("Error: failed to render report: {e}");
                    return Err(1);
                }
            }
        }
        Command::Migrate { config } => {
            let cfg = load_config_or_default(config.as_deref());
            if let Err(err) = run_migrate(&cfg).await {
                eprintln!("Error: migration failed: {err}");
                return Err(1);
            }
        }
        Command::History { command } => {
            let cfg = load_config_or_default(history_config(&command).as_deref());
            run_history(command, &cfg).await?;
        }
    }
    Ok(())
}

fn load_config_or_default(path: Option<&std::path::Path>) -> Config {
    load_config(path).unwrap_or_else(|_| Config::default())
}

fn history_config(command: &HistoryCommand) -> Option<std::path::PathBuf> {
    match command {
        HistoryCommand::List { config, .. } | HistoryCommand::Show { config, .. } => config.clone(),
    }
}

enum DiagnoseError {
    Fetch(Box<dyn std::error::Error>),
    Save(Box<dyn std::error::Error>),
    Render(Box<dyn std::error::Error>),
}

async fn run_diagnose(
    url: &str,
    since: &str,
    model: Option<&str>,
    output: Format,
    verbose: bool,
    save: bool,
    config: &Config,
) -> Result<(), DiagnoseError> {
    let registry = build_registry(config);
    let provider = resolve_provider(url, 10.0, since, model)
        .await
        .map_err(|e| DiagnoseError::Fetch(e.into()))?;
    let result = diagnose(provider.as_ref(), &registry, since, model, config)
        .await
        .map_err(|e| DiagnoseError::Fetch(e.into()))?;
    let report = Report::new(result);

    let stdout = std::io::stdout();
    let opts = RenderOptions {
        verbose,
        width: terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80),
        color: stdout.is_terminal(),
    };

    match output {
        Format::Text => print!("{}", text::render(&report, &opts)),
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json::render(&report, verbose))
                .map_err(|e| DiagnoseError::Render(e.into()))?
        ),
    }

    if save {
        let store = SqliteHistoryStore::connect(&config.database.url)
            .await
            .map_err(|e| DiagnoseError::Save(e.into()))?;
        let id = store
            .save(&report.diagnosis)
            .await
            .map_err(|e| DiagnoseError::Save(e.into()))?;
        eprintln!("Saved run: {id}");
    }

    Ok(())
}

async fn run_migrate(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let _store = SqliteHistoryStore::connect(&config.database.url).await?;
    println!("Database migrated successfully.");
    Ok(())
}

fn firing_ids(result: &DiagnosisResult) -> Vec<String> {
    result
        .checks
        .iter()
        .filter(|c| c.finding.is_some())
        .map(|c| c.id.clone())
        .collect()
}

fn transition_label(prev: Option<&DiagnosisResult>, curr: &DiagnosisResult) -> Option<String> {
    match prev {
        None => Some("initial".to_string()),
        Some(prev) => {
            if prev.health() != curr.health() {
                Some(format!("{} → {}", prev.health(), curr.health()))
            } else if firing_ids(prev) != firing_ids(curr) {
                Some("rules changed".to_string())
            } else {
                None
            }
        }
    }
}

async fn run_watch(
    url: &str,
    since: &str,
    model: Option<&str>,
    output: Format,
    verbose: bool,
    save: bool,
    config: &Config,
) -> Result<(), DiagnoseError> {
    let registry = build_registry(config);
    let provider = resolve_provider(url, 10.0, since, model)
        .await
        .map_err(|e| DiagnoseError::Fetch(e.into()))?;

    let store = if save {
        Some(
            SqliteHistoryStore::connect(&config.database.url)
                .await
                .map_err(|e| DiagnoseError::Save(e.into()))?,
        )
    } else {
        None
    };

    let stdout = std::io::stdout();
    let opts = RenderOptions {
        verbose,
        width: terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80),
        color: stdout.is_terminal(),
    };

    let mut prev: Option<DiagnosisResult> = None;
    loop {
        let result = diagnose(provider.as_ref(), &registry, since, model, config)
            .await
            .map_err(|e| DiagnoseError::Fetch(e.into()))?;
        let report = Report::new(result.clone());

        match output {
            Format::Text => {
                use std::io::Write;
                print!("\x1b[2J\x1b[H");
                print!("{}", text::render(&report, &opts));
                std::io::stdout().flush().ok();
            }
            Format::Json => {
                let json = serde_json::to_string(&json::render(&report, verbose))
                    .map_err(|e| DiagnoseError::Render(e.into()))?;
                println!("{json}");
            }
        }

        if let Some(ref store) = store {
            if let Some(label) = transition_label(prev.as_ref(), &result) {
                let id = store
                    .save(&result)
                    .await
                    .map_err(|e| DiagnoseError::Save(e.into()))?;
                eprintln!("Saved run: {id} ({label})");
            }
        }

        prev = Some(result);
        tokio::select! {
            _ = tokio::time::sleep(WATCH_INTERVAL) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

async fn run_history(command: HistoryCommand, config: &Config) -> Result<(), i32> {
    let store = match SqliteHistoryStore::connect(&config.database.url).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            return Err(1);
        }
    };
    match command {
        HistoryCommand::List {
            output, verbose, ..
        } => {
            let runs = match store.list().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return Err(1);
                }
            };
            match output {
                Format::Json => match serde_json::to_string(&runs) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return Err(1);
                    }
                },
                Format::Text => {
                    if runs.is_empty() {
                        println!("No saved diagnosis runs found.");
                    } else {
                        print_history_table(&runs, verbose);
                    }
                }
            }
        }
        HistoryCommand::Show {
            run_id,
            output,
            verbose,
            ..
        } => {
            let result = match store.get(&run_id).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return Err(1);
                }
            };
            let Some(result) = result else {
                eprintln!("Error: run {run_id} not found.");
                return Err(1);
            };
            let report = Report::new(result);
            let stdout = std::io::stdout();
            let opts = RenderOptions {
                verbose,
                width: terminal_size::terminal_size()
                    .map(|(w, _)| w.0 as usize)
                    .unwrap_or(80),
                color: stdout.is_terminal(),
            };
            match output {
                Format::Text => print!("{}", text::render(&report, &opts)),
                Format::Json => match serde_json::to_string_pretty(&json::render(&report, verbose))
                {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return Err(1);
                    }
                },
            }
        }
    }
    Ok(())
}

fn print_history_table(runs: &[vllm_doctor::stores::RunSummary], verbose: bool) {
    let stdout = std::io::stdout();
    let color = stdout.is_terminal();

    let mut table = Table::new();
    table
        .load_preset(UTF8_BORDERS_ONLY)
        .set_content_arrangement(ContentArrangement::Disabled);

    let mut header = vec!["Run ID", "Time", "Model"];
    if verbose {
        header.push("Mode");
    }
    header.push("Health");
    header.push("Fired");
    table.set_header(header);

    for run in runs {
        let saved = run.saved_at.format("%Y-%m-%d %H:%M").to_string();
        let model = run.model_name.clone().unwrap_or_else(|| "—".to_string());
        let mut row = vec![Cell::new(run.run_id), Cell::new(saved), Cell::new(model)];
        if verbose {
            row.push(Cell::new(run.client_mode.to_string()));
        }
        let health_cell = Cell::new(run.health.to_string());
        let health_cell = if color {
            health_cell.fg(health_to_comfy(run.health))
        } else {
            health_cell
        };
        row.push(health_cell);
        row.push(Cell::new(run.fired_count));
        table.add_row(row);
    }

    println!("{table}");
}

fn health_to_comfy(health: Health) -> Color {
    match health {
        Health::Ok => Color::Green,
        Health::Info => Color::Blue,
        Health::Warning => Color::Yellow,
        Health::Critical => Color::Red,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllm_doctor::metrics::MetricSeriesSnapshot;
    use vllm_doctor::models::{Confidence, Finding, RuleResult, Severity};

    fn result_with(findings: Vec<Option<Finding>>) -> DiagnosisResult {
        let checks = findings
            .into_iter()
            .enumerate()
            .map(|(i, finding)| RuleResult {
                id: format!("rule-{i}"),
                name: format!("Rule {i}"),
                title: format!("Rule {i}"),
                severity: Severity::Warning,
                finding,
            })
            .collect();
        DiagnosisResult {
            context: vllm_doctor::models::DiagnosisContext::new("5m"),
            metric_series: MetricSeriesSnapshot::default(),
            checks,
        }
    }

    fn finding(severity: Severity) -> Finding {
        Finding {
            severity,
            confidence: Confidence::Medium,
            title: "Test".to_string(),
            summary: "test".to_string(),
            signals: vec![],
            evidence: vec![],
            likely_causes: vec![],
            recommendations: vec![],
            related_metrics: vec![],
        }
    }

    #[test]
    fn firing_ids_extracts_only_firing_checks() {
        let result = result_with(vec![Some(finding(Severity::Warning)), None]);
        let ids = firing_ids(&result);
        assert_eq!(ids, vec!["rule-0".to_string()]);
    }

    #[test]
    fn transition_label_initial() {
        let curr = result_with(vec![Some(finding(Severity::Warning))]);
        assert_eq!(transition_label(None, &curr), Some("initial".to_string()));
    }

    #[test]
    fn transition_label_health_change() {
        let prev = result_with(vec![Some(finding(Severity::Warning))]);
        let curr = result_with(vec![Some(finding(Severity::Critical))]);
        assert_eq!(
            transition_label(Some(&prev), &curr),
            Some("warning → critical".to_string())
        );
    }

    #[test]
    fn transition_label_rules_changed() {
        let prev = result_with(vec![Some(finding(Severity::Warning)), None]);
        let curr = result_with(vec![
            Some(finding(Severity::Warning)),
            Some(finding(Severity::Warning)),
        ]);
        assert_eq!(
            transition_label(Some(&prev), &curr),
            Some("rules changed".to_string())
        );
    }

    #[test]
    fn transition_label_no_change() {
        let prev = result_with(vec![Some(finding(Severity::Warning))]);
        let curr = result_with(vec![Some(finding(Severity::Warning))]);
        assert_eq!(transition_label(Some(&prev), &curr), None);
    }
}
