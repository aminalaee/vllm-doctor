use std::path::PathBuf;
use std::time::Duration;

use vllm_doctor::cli::{DiagnoseArgs, Format};
use vllm_doctor::clients::ConnectionOptions;
use vllm_doctor::models::{DiagnosisResult, Health, TargetMetadata};
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::runner::{DiagnoseRequest, DiagnoseRunner, RunnerError};
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

use super::{
    CommandResult, EXIT_ERROR, EXIT_UNHEALTHY, load_config_or_exit, render_options, watch,
};

pub(super) async fn run(args: DiagnoseArgs) -> CommandResult {
    let DiagnoseArgs {
        url,
        since,
        model,
        output,
        verbose,
        save,
        watch: watch_mode,
        interval,
        timeout,
        headers,
        ca_cert,
        config,
    } = args;

    let config = load_config_or_exit(config.as_deref())?;
    let connection = build_connection_options(&headers, ca_cert);
    let render = render_options(verbose);
    let database_url = config.database.url.clone();
    let target = TargetMetadata {
        id: config.target.id.clone(),
        engine: config.target.engine,
        engine_version: config.target.engine_version.clone(),
        environment: config.target.environment.clone(),
    };
    let request = DiagnoseRequest {
        url: url.clone(),
        since,
        model,
        timeout,
        interval: Duration::from_secs_f64(interval),
        config,
        conn_opts: connection,
        target,
    };

    if watch_mode {
        watch::run(
            request,
            watch::Options {
                output,
                verbose,
                save,
                render: &render,
            },
        )
        .await;
        return Ok(());
    }

    let runner = DiagnoseRunner::new(request).await.map_err(|error| {
        print_fetch_error(&url, error);
        EXIT_ERROR
    })?;
    let result = runner.run_once().await.map_err(|error| {
        print_fetch_error(&url, error);
        EXIT_ERROR
    })?;
    let report = Report::new(result.clone());
    print_report(&report, output, verbose, &render);

    if save {
        match save_run(&database_url, &result).await {
            Ok(id) => eprintln!("Saved run: {id}"),
            Err(error) => {
                eprintln!("Error: failed to save run: {error}");
                return Err(EXIT_ERROR);
            }
        }
    }

    let code = exit_code_for(Some(report.health()));
    if code == 0 { Ok(()) } else { Err(code) }
}

fn print_fetch_error(url: &str, error: RunnerError) {
    let RunnerError::Fetch(error) = error;
    eprintln!("Error: could not read metrics from {url}: {error}");
}

fn print_report(report: &Report, output: Format, verbose: bool, options: &RenderOptions) {
    match output {
        Format::Text => print!("{}", text::render(report, options)),
        Format::Json => {
            let rendered = serde_json::to_string_pretty(&json::render(report, verbose))
                .unwrap_or_else(|error| {
                    eprintln!("Error: failed to render report: {error}");
                    String::new()
                });
            println!("{rendered}");
        }
    }
}

async fn save_run(
    database_url: &str,
    result: &DiagnosisResult,
) -> Result<String, Box<dyn std::error::Error>> {
    let store = SqliteHistoryStore::connect(database_url).await?;
    let id = store.save(result).await?;
    Ok(id.to_string())
}

fn build_connection_options(
    headers: &[(String, String)],
    ca_cert: Option<PathBuf>,
) -> ConnectionOptions {
    let mut options = ConnectionOptions::new();
    for (name, value) in headers {
        options = options.with_header(name, value);
    }
    if let Some(path) = ca_cert {
        options = options.with_ca_cert(path);
    }
    options
}

fn exit_code_for(health: Option<Health>) -> i32 {
    match health {
        Some(Health::Critical) => EXIT_UNHEALTHY,
        _ => 0,
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
