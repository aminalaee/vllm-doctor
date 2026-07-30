use std::io::IsTerminal;
use std::path::Path;

use crate::cli::args::{Format, HistoryCommand};
use crate::cli::reports::{Report, json, text};
use crate::cli::stores::{HistoryStore, RunSummary, SqliteHistoryStore};
use crate::core::models::Health;
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_BORDERS_ONLY};

use super::{CommandResult, EXIT_ERROR, load_config_or_exit, render_options};

pub(super) async fn run(command: HistoryCommand) -> CommandResult {
    let config = load_config_or_exit(config_path(&command))?;
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .map_err(|error| {
            eprintln!("Error: {error}");
            EXIT_ERROR
        })?;

    match command {
        HistoryCommand::List {
            output, verbose, ..
        } => list(&store, output, verbose).await,
        HistoryCommand::Show {
            run_id,
            output,
            verbose,
            ..
        } => show(&store, &run_id, output, verbose).await,
    }
}

fn config_path(command: &HistoryCommand) -> Option<&Path> {
    match command {
        HistoryCommand::List { config, .. } | HistoryCommand::Show { config, .. } => {
            config.as_deref()
        }
    }
}

async fn list(store: &SqliteHistoryStore, output: Format, verbose: bool) -> CommandResult {
    let runs = store.list().await.map_err(|error| {
        eprintln!("Error: {error}");
        EXIT_ERROR
    })?;

    match output {
        Format::Json => match serde_json::to_string(&runs) {
            Ok(rendered) => println!("{rendered}"),
            Err(error) => {
                eprintln!("Error: {error}");
                return Err(EXIT_ERROR);
            }
        },
        Format::Text if runs.is_empty() => println!("No saved diagnosis runs found."),
        Format::Text => print_table(&runs, verbose),
    }
    Ok(())
}

async fn show(
    store: &SqliteHistoryStore,
    run_id: &str,
    output: Format,
    verbose: bool,
) -> CommandResult {
    let result = store.get(run_id).await.map_err(|error| {
        eprintln!("Error: {error}");
        EXIT_ERROR
    })?;
    let Some(result) = result else {
        eprintln!("Error: run {run_id} not found.");
        return Err(EXIT_ERROR);
    };

    let report = Report::new(result);
    match output {
        Format::Text => print!("{}", text::render(&report, &render_options(verbose))),
        Format::Json => match serde_json::to_string_pretty(&json::render(&report, verbose)) {
            Ok(rendered) => println!("{rendered}"),
            Err(error) => {
                eprintln!("Error: {error}");
                return Err(EXIT_ERROR);
            }
        },
    }
    Ok(())
}

fn print_table(runs: &[RunSummary], verbose: bool) {
    let color = std::io::stdout().is_terminal();
    let mut table = Table::new();
    table
        .load_preset(UTF8_BORDERS_ONLY)
        .set_content_arrangement(ContentArrangement::Disabled);

    let mut header = vec!["Run ID", "Time", "Model"];
    if verbose {
        header.extend(["Source", "Engine", "Target"]);
    }
    header.extend(["Health", "Fired"]);
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
        let health = Cell::new(run.health.to_string());
        row.push(if color {
            health.fg(health_color(run.health))
        } else {
            health
        });
        row.push(Cell::new(run.fired_count));
        table.add_row(row);
    }

    println!("{table}");
}

fn health_color(health: Health) -> Color {
    match health {
        Health::Ok => Color::Green,
        Health::Info => Color::Blue,
        Health::Warning => Color::Yellow,
        Health::Critical => Color::Red,
    }
}
