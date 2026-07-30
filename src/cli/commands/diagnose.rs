use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::cli::args::{DiagnoseArgs, Format};
use crate::cli::clients::ConnectionOptions;
use crate::cli::reports::{RenderOptions, Report, json, text};
use crate::cli::runner::{DiagnoseRequest, DiagnoseRunner, RunnerError};
use crate::cli::stores::{HistoryStore, SqliteHistoryStore};
use crate::cli::upload::{UploadClient, UploadOutcome, ensure_agent_id, resolve_token};
use crate::core::models::{DiagnosisResult, Health, TargetMetadata};
use crate::core::observations::parse_window_seconds;
use crate::core::observations::v1::{ObservationBuildContext, build_observation};

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
        listen,
        interval,
        timeout,
        headers,
        ca_cert,
        config: config_path,
        upload,
    } = args;

    let config = load_config_or_exit(config_path.as_deref())?;
    let listen = resolve_listen(listen, config.agent.listen);
    let connection = build_connection_options(&headers, ca_cert);
    let render = render_options(verbose);
    let database_url = config.database.url.clone();
    let upload_config = config.upload.clone();
    let agent_id = config.agent.id.clone();
    let upload = upload || upload_config.enabled;
    let target = TargetMetadata {
        id: config.target.id.clone(),
        engine: config.target.engine,
        engine_version: config.target.engine_version.clone(),
        environment: config.target.environment.clone(),
    };
    let request = DiagnoseRequest {
        url: url.clone(),
        since: since.clone(),
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
                listen,
                render: &render,
                upload,
                upload_config,
                agent_id,
            },
        )
        .await
        .map_err(|error| {
            eprintln!("Error: watch mode failed: {error:#}");
            EXIT_ERROR
        })?;
        return Ok(());
    }

    let runner = DiagnoseRunner::new(request.clone())
        .await
        .map_err(|error| {
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

    if upload {
        let agent_id = ensure_agent_id(agent_id.as_deref()).map_err(|error| {
            eprintln!("Error: could not resolve agent identity: {error}");
            EXIT_ERROR
        })?;
        let client = UploadClient::new(&upload_config).map_err(|error| {
            eprintln!("Error: could not initialize upload: {error}");
            EXIT_ERROR
        })?;
        if let Err(error) = upload_diagnosis(&result, &since, &agent_id, &client, true).await {
            eprintln!("Error: upload failed: {error}");
            return Err(EXIT_ERROR);
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

/// Build an observation from the diagnosis result and upload it. Performs one
/// retry with a short backoff on retryable errors. Returns the outcome label
/// on success.
pub(super) async fn upload_diagnosis(
    result: &DiagnosisResult,
    since: &str,
    agent_id: &str,
    client: &UploadClient,
    retry_once: bool,
) -> Result<&'static str, crate::cli::upload::UploadError> {
    let observation = build_upload_observation(result, since, agent_id)?;

    let token = resolve_token()?;

    match client.upload(&observation, &token).await {
        Ok(UploadOutcome::Created) => {
            eprintln!("Uploaded observation (created)");
            Ok("created")
        }
        Ok(UploadOutcome::Deduplicated) => {
            eprintln!("Uploaded observation (deduplicated)");
            Ok("deduplicated")
        }
        Err(error) if retry_once && error.is_retryable() => {
            eprintln!("Warning: upload failed ({error}); retrying once");
            tokio::time::sleep(Duration::from_millis(500)).await;
            match client.upload(&observation, &token).await {
                Ok(UploadOutcome::Created) => {
                    eprintln!("Uploaded observation (created on retry)");
                    Ok("created")
                }
                Ok(UploadOutcome::Deduplicated) => {
                    eprintln!("Uploaded observation (deduplicated on retry)");
                    Ok("deduplicated")
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn build_upload_observation(
    result: &DiagnosisResult,
    since: &str,
    agent_id: &str,
) -> Result<crate::core::observations::v1::ObservationV1, crate::cli::upload::UploadError> {
    let window_seconds = parse_window_seconds(since)
        .map_err(|error| crate::cli::upload::UploadError::InvalidWindow(error.to_string()))?;
    let context = ObservationBuildContext {
        event_id: uuid::Uuid::now_v7(),
        observed_at: chrono::Utc::now(),
        agent_id: agent_id.to_string(),
        agent_version: crate::version().to_string(),
        local_rule_pack: crate::version().to_string(),
    };
    build_observation(result, &context, window_seconds)
        .map_err(|error| crate::cli::upload::UploadError::Build(error.to_string()))
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

fn resolve_listen(cli: Option<SocketAddr>, config: Option<SocketAddr>) -> Option<SocketAddr> {
    cli.or(config)
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

    #[test]
    fn cli_listen_overrides_config() {
        let cli = "127.0.0.1:9091".parse().unwrap();
        let config = "127.0.0.1:9092".parse().unwrap();
        assert_eq!(resolve_listen(Some(cli), Some(config)), Some(cli));
        assert_eq!(resolve_listen(None, Some(config)), Some(config));
        assert_eq!(resolve_listen(None, None), None);
    }
}
