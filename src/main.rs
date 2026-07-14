use std::io::IsTerminal;
use std::time::Duration;

use clap::Parser;
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_BORDERS_ONLY};

use vllm_doctor::cli::{Args, Command, Format, HistoryCommand};
use vllm_doctor::clients::ConnectionOptions;
use vllm_doctor::config::{Config, load_config};
use vllm_doctor::diagnosis::diagnose;
use vllm_doctor::models::{DiagnosisResult, Health};
use vllm_doctor::providers::resolve_provider;
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::rules::build_registry;
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

const EXIT_UNHEALTHY: i32 = 1;
const EXIT_ERROR: i32 = 2;

fn exit_code_for(health: Option<Health>) -> i32 {
    match health {
        Some(Health::Critical) => EXIT_UNHEALTHY,
        _ => 0,
    }
}

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
            interval,
            timeout,
            headers,
            ca_cert,
            config,
        } => {
            let cfg = load_config_or_exit(config.as_deref())?;
            let conn_opts = build_connection_options(&headers, ca_cert);
            let params = DiagnoseParams {
                url: &url,
                since: &since,
                model: model.as_deref(),
                output,
                verbose,
                save,
                timeout,
                interval: Duration::from_secs_f64(interval),
                config: &cfg,
                conn_opts: &conn_opts,
            };
            let result = if watch {
                run_watch(params).await.map(|()| None)
            } else {
                run_diagnose(params).await.map(Some)
            };
            match result {
                Ok(health) => {
                    let code = exit_code_for(health);
                    if code != 0 {
                        return Err(code);
                    }
                }
                Err(DiagnoseError::Fetch(e)) => {
                    eprintln!("Error: could not read metrics from {url}: {e}");
                    return Err(EXIT_ERROR);
                }
                Err(DiagnoseError::Save(e)) => {
                    eprintln!("Error: failed to save run: {e}");
                    return Err(EXIT_ERROR);
                }
                Err(DiagnoseError::Render(e)) => {
                    eprintln!("Error: failed to render report: {e}");
                    return Err(EXIT_ERROR);
                }
            }
        }
        Command::Migrate { config } => {
            let cfg = load_config_or_exit(config.as_deref())?;
            if let Err(err) = run_migrate(&cfg).await {
                eprintln!("Error: migration failed: {err}");
                return Err(EXIT_ERROR);
            }
        }
        Command::History { command } => {
            let cfg = load_config_or_exit(history_config(&command).as_deref())?;
            run_history(command, &cfg).await?;
        }
    }
    Ok(())
}

/// Load configuration, exiting with a clear error rather than silently falling
/// back to defaults. A missing or malformed config (an explicit `--config` path,
/// or a `vllm-doctor.toml` in scope) is a user error worth surfacing; absent
/// discovery files still yield defaults from within `load_config`.
fn load_config_or_exit(path: Option<&std::path::Path>) -> Result<Config, i32> {
    load_config(path).map_err(|err| {
        eprintln!("Error: could not load config: {err}");
        EXIT_ERROR
    })
}

fn build_connection_options(
    headers: &[(String, String)],
    ca_cert: Option<std::path::PathBuf>,
) -> ConnectionOptions {
    let mut opts = ConnectionOptions::new();
    for (name, value) in headers {
        opts = opts.with_header(name, value);
    }
    if let Some(path) = ca_cert {
        opts = opts.with_ca_cert(path);
    }
    opts
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

#[derive(Clone, Copy)]
struct DiagnoseParams<'a> {
    url: &'a str,
    since: &'a str,
    model: Option<&'a str>,
    output: Format,
    verbose: bool,
    save: bool,
    timeout: f64,
    interval: Duration,
    config: &'a Config,
    conn_opts: &'a ConnectionOptions,
}

async fn run_diagnose(params: DiagnoseParams<'_>) -> Result<Health, DiagnoseError> {
    let DiagnoseParams {
        url,
        since,
        model,
        output,
        verbose,
        save,
        timeout,
        interval: _,
        config,
        conn_opts,
    } = params;
    let registry = build_registry(config);
    let provider = resolve_provider(url, timeout, conn_opts, since, model)
        .await
        .map_err(|e| DiagnoseError::Fetch(e.into()))?;
    let result = diagnose(provider.as_ref(), &registry, since, model, config)
        .await
        .map_err(|e| DiagnoseError::Fetch(e.into()))?;
    let report = Report::new(result);

    let opts = RenderOptions {
        verbose,
        width: terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80),
        color: std::io::stdout().is_terminal(),
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

    Ok(report.health())
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

async fn run_watch(params: DiagnoseParams<'_>) -> Result<(), DiagnoseError> {
    let DiagnoseParams {
        url,
        since,
        model,
        output,
        verbose,
        save,
        timeout,
        interval,
        config,
        conn_opts,
    } = params;
    let registry = build_registry(config);
    let provider = resolve_provider(url, timeout, conn_opts, since, model)
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

    let opts = RenderOptions {
        verbose,
        width: terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80),
        color: std::io::stdout().is_terminal(),
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
            _ = tokio::time::sleep(interval) => {}
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
            return Err(EXIT_ERROR);
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
                    return Err(EXIT_ERROR);
                }
            };
            match output {
                Format::Json => match serde_json::to_string(&runs) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return Err(EXIT_ERROR);
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
                    return Err(EXIT_ERROR);
                }
            };
            let Some(result) = result else {
                eprintln!("Error: run {run_id} not found.");
                return Err(EXIT_ERROR);
            };
            let report = Report::new(result);
            let opts = RenderOptions {
                verbose,
                width: terminal_size::terminal_size()
                    .map(|(w, _)| w.0 as usize)
                    .unwrap_or(80),
                color: std::io::stdout().is_terminal(),
            };
            match output {
                Format::Text => print!("{}", text::render(&report, &opts)),
                Format::Json => match serde_json::to_string_pretty(&json::render(&report, verbose))
                {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return Err(EXIT_ERROR);
                    }
                },
            }
        }
    }
    Ok(())
}

fn print_history_table(runs: &[vllm_doctor::stores::RunSummary], verbose: bool) {
    let color = std::io::stdout().is_terminal();

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
        DiagnosisResult::new(
            vllm_doctor::models::DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            checks,
        )
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

    #[test]
    fn exit_code_unhealthy_only_on_critical() {
        assert_eq!(exit_code_for(Some(Health::Critical)), EXIT_UNHEALTHY);
        assert_eq!(exit_code_for(Some(Health::Warning)), 0);
        assert_eq!(exit_code_for(Some(Health::Info)), 0);
        assert_eq!(exit_code_for(Some(Health::Ok)), 0);
        assert_eq!(exit_code_for(None), 0);
    }
}
