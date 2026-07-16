use std::io::{IsTerminal, Write};
use std::time::Duration;

use clap::Parser;
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_BORDERS_ONLY};

use vllm_doctor::cli::{Args, Command, Format, HistoryCommand};
use vllm_doctor::clients::ConnectionOptions;
use vllm_doctor::config::{Config, load_config};
use vllm_doctor::models::TargetMetadata;
use vllm_doctor::models::{DiagnosisResult, Health};
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::runner::{DiagnoseRequest, DiagnoseRunner, RunnerError, transition_label};
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
            let render_opts = RenderOptions {
                verbose,
                width: terminal_size::terminal_size()
                    .map(|(w, _)| w.0 as usize)
                    .unwrap_or(80),
                color: std::io::stdout().is_terminal(),
            };
            let db_url = cfg.database.url.clone();
            let target = TargetMetadata {
                id: cfg.target.id.clone(),
                engine: cfg.target.engine,
                engine_version: cfg.target.engine_version.clone(),
                environment: cfg.target.environment.clone(),
            };
            let request = DiagnoseRequest {
                url: url.clone(),
                since: since.clone(),
                model: model.clone(),
                timeout,
                interval: Duration::from_secs_f64(interval),
                config: cfg,
                conn_opts,
                target,
            };
            let runner = match DiagnoseRunner::new(request).await {
                Ok(r) => r,
                Err(RunnerError::Fetch(e)) => {
                    eprintln!("Error: could not read metrics from {url}: {e}");
                    return Err(EXIT_ERROR);
                }
            };
            if watch {
                if let Err(e) = run_watch(&runner, output, verbose, save, &render_opts).await {
                    eprintln!("Error: could not read metrics from {url}: {e}");
                    return Err(EXIT_ERROR);
                }
            } else {
                let result = match runner.run_once().await {
                    Ok(r) => r,
                    Err(RunnerError::Fetch(e)) => {
                        eprintln!("Error: could not read metrics from {url}: {e}");
                        return Err(EXIT_ERROR);
                    }
                };
                let report = Report::new(result.clone());
                print_report(&report, output, verbose, &render_opts);
                if save {
                    match save_run(&db_url, &result).await {
                        Ok(id) => eprintln!("Saved run: {id}"),
                        Err(e) => {
                            eprintln!("Error: failed to save run: {e}");
                            return Err(EXIT_ERROR);
                        }
                    }
                }
                let code = exit_code_for(Some(report.health()));
                if code != 0 {
                    return Err(code);
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

fn print_report(report: &Report, output: Format, verbose: bool, opts: &RenderOptions) {
    match output {
        Format::Text => print!("{}", text::render(report, opts)),
        Format::Json => {
            let json =
                serde_json::to_string_pretty(&json::render(report, verbose)).unwrap_or_else(|e| {
                    eprintln!("Error: failed to render report: {e}");
                    String::new()
                });
            println!("{json}");
        }
    }
}

async fn save_run(
    db_url: &str,
    result: &DiagnosisResult,
) -> Result<String, Box<dyn std::error::Error>> {
    let store = SqliteHistoryStore::connect(db_url).await?;
    let id = store.save(result).await?;
    Ok(id.to_string())
}

async fn run_watch(
    runner: &DiagnoseRunner,
    output: Format,
    verbose: bool,
    save: bool,
    opts: &RenderOptions,
) -> Result<(), RunnerError> {
    let store = if save {
        match SqliteHistoryStore::connect(&runner.config().database.url).await {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("Error: failed to connect to history database: {e}");
                None
            }
        }
    } else {
        None
    };

    let mut prev: Option<DiagnosisResult> = None;
    loop {
        let result = runner.run_once().await?;
        let report = Report::new(result.clone());

        match output {
            Format::Text => {
                print!("\x1b[2J\x1b[H");
                print!("{}", text::render(&report, opts));
                std::io::stdout().flush().ok();
            }
            Format::Json => match serde_json::to_string(&json::render(&report, verbose)) {
                Ok(json) => println!("{json}"),
                Err(e) => eprintln!("Error: failed to render report: {e}"),
            },
        }

        if let Some(ref store) = store {
            if let Some(label) = transition_label(prev.as_ref(), &result) {
                match store.save(&result).await {
                    Ok(id) => eprintln!("Saved run: {id} ({label})"),
                    Err(e) => eprintln!("Error: failed to save run: {e}"),
                }
            }
        }

        prev = Some(result);
        tokio::select! {
            _ = tokio::time::sleep(runner.interval()) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

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

async fn run_migrate(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let _store = SqliteHistoryStore::connect(&config.database.url).await?;
    println!("Database migrated successfully.");
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
        header.push("Source");
        header.push("Engine");
        header.push("Target");
    }
    header.push("Health");
    header.push("Fired");
    table.set_header(header);

    for run in runs {
        let saved = run.saved_at.format("%Y-%m-%d %H:%M").to_string();
        let model = run.model_name.clone().unwrap_or_else(|| "—".to_string());
        let mut row = vec![Cell::new(run.run_id), Cell::new(saved), Cell::new(model)];
        if verbose {
            row.push(Cell::new(run.metrics_source.to_string()));
            row.push(Cell::new(run.engine.to_string()));
            row.push(Cell::new(run.target_id.as_deref().unwrap_or("—")));
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

    #[test]
    fn exit_code_unhealthy_only_on_critical() {
        assert_eq!(exit_code_for(Some(Health::Critical)), EXIT_UNHEALTHY);
        assert_eq!(exit_code_for(Some(Health::Warning)), 0);
        assert_eq!(exit_code_for(Some(Health::Info)), 0);
        assert_eq!(exit_code_for(Some(Health::Ok)), 0);
        assert_eq!(exit_code_for(None), 0);
    }
}
