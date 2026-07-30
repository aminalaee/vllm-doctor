//! Continuous diagnosis orchestration for the CLI.
use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cli::args::Format;
use crate::cli::observability::AgentState;
use crate::cli::reports::{RenderOptions, Report, json, text};
use crate::cli::runner::{DiagnoseRequest, DiagnoseRunner, RunnerError, transition_label};
use crate::cli::stores::{HistoryStore, SqliteHistoryStore};
use crate::core::models::DiagnosisResult;
use anyhow::{Context, anyhow};
use tokio::sync::watch as shutdown;

use super::observability;

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const JITTER_PERCENT: u64 = 20;

pub(super) struct Options<'a> {
    pub(super) output: Format,
    pub(super) verbose: bool,
    pub(super) save: bool,
    pub(super) listen: Option<SocketAddr>,
    pub(super) render: &'a RenderOptions,
    pub(super) upload: bool,
    pub(super) config_path: Option<std::path::PathBuf>,
    pub(super) upload_config: Option<crate::cli::config::UploadConfig>,
    pub(super) agent_id: Option<String>,
}

pub(super) async fn run(request: DiagnoseRequest, options: Options<'_>) -> anyhow::Result<()> {
    let state = options
        .listen
        .map(|_| Arc::new(AgentState::new(request.target.clone())));
    let listener = if let Some(address) = options.listen {
        Some(
            tokio::net::TcpListener::bind(address)
                .await
                .with_context(|| format!("could not bind observability listener to {address}"))?,
        )
    } else {
        None
    };
    let (shutdown_tx, shutdown_rx) = shutdown::channel(false);
    let watch = watch_loop(request, options, state.clone(), shutdown_rx.clone());
    tokio::pin!(watch);

    let Some(listener) = listener else {
        return tokio::select! {
            result = &mut watch => result,
            signal = termination_signal() => {
                signal?;
                let _ = shutdown_tx.send(true);
                watch.await
            }
        };
    };

    let server_state = state.ok_or_else(|| anyhow!("observability state was not initialized"))?;
    let server_shutdown = wait_for_shutdown(shutdown_rx);
    let server = observability::serve(listener, server_state, server_shutdown);
    tokio::pin!(server);

    tokio::select! {
        result = &mut watch => {
            let _ = shutdown_tx.send(true);
            server.await.context("observability server failed during shutdown")?;
            result
        }
        result = &mut server => {
            let _ = shutdown_tx.send(true);
            watch.await?;
            result.context("observability server failed")?;
            Err(anyhow!("observability server stopped unexpectedly"))
        }
        signal = termination_signal() => {
            signal?;
            let _ = shutdown_tx.send(true);
            let (watch_result, server_result) = tokio::join!(watch, server);
            watch_result?;
            server_result.context("observability server failed during shutdown")?;
            Ok(())
        }
    }
}

async fn termination_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate =
            signal(SignalKind::terminate()).context("failed to listen for SIGTERM")?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.context("failed to listen for Ctrl-C")
            }
            signal = terminate.recv() => {
                signal
                    .map(|_| ())
                    .ok_or_else(|| anyhow!("SIGTERM listener closed unexpectedly"))
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl-C")
    }
}

async fn watch_loop(
    request: DiagnoseRequest,
    options: Options<'_>,
    state: Option<Arc<AgentState>>,
    mut shutdown: shutdown::Receiver<bool>,
) -> anyhow::Result<()> {
    let store = if options.save {
        match SqliteHistoryStore::connect(&request.config.database.url).await {
            Ok(store) => Some(store),
            Err(error) => {
                eprintln!("Error: failed to connect to history database: {error}");
                None
            }
        }
    } else {
        None
    };

    let mut schedule = WatchSchedule::new(request.interval);
    let Some(runner) = connect(&request, &mut schedule, state.as_deref(), &mut shutdown).await
    else {
        return Ok(());
    };
    let clear_terminal = should_clear_output(options.output, std::io::stdout().is_terminal());
    let mut previous: Option<DiagnosisResult> = None;

    loop {
        let attempt = tokio::select! {
            result = runner.run_once() => result,
            _ = wait_for_shutdown(shutdown.clone()) => break,
        };

        let delay = match attempt {
            Ok(result) => {
                if let Some(state) = state.as_deref() {
                    state.record_success(&result, chrono::Utc::now());
                }
                if let Err(error) =
                    print_report(&Report::new(result.clone()), &options, clear_terminal)
                {
                    if error.kind() != std::io::ErrorKind::BrokenPipe {
                        eprintln!("Error: failed to write report output: {error}");
                    }
                    break;
                }

                if let Some(ref store) = store {
                    if let Some(label) = transition_label(previous.as_ref(), &result) {
                        match store.save(&result).await {
                            Ok(id) => eprintln!("Saved run: {id} ({label})"),
                            Err(error) => eprintln!("Error: failed to save run: {error}"),
                        }
                    }
                }

                if options.upload {
                    if let Err(error) =
                        upload_in_watch(&result, &options, runner.config(), runner.interval()).await
                    {
                        eprintln!("Error: upload failed: {error}");
                        if let Some(state) = state.as_deref() {
                            state.record_upload_error();
                        }
                    }
                }

                previous = Some(result);
                schedule.success_delay()
            }
            Err(RunnerError::Fetch(error)) => {
                if let Some(state) = state.as_deref() {
                    state.record_error();
                }
                retry_delay(&mut schedule, "collection", &error)
            }
        };

        if !wait(delay, &mut shutdown).await {
            break;
        }
    }
    Ok(())
}

async fn connect(
    request: &DiagnoseRequest,
    schedule: &mut WatchSchedule,
    state: Option<&AgentState>,
    shutdown: &mut shutdown::Receiver<bool>,
) -> Option<DiagnoseRunner> {
    loop {
        let attempt = tokio::select! {
            result = DiagnoseRunner::new(request.clone()) => result,
            _ = wait_for_shutdown(shutdown.clone()) => return None,
        };

        match attempt {
            Ok(runner) => return Some(runner),
            Err(RunnerError::Fetch(error)) => {
                if let Some(state) = state {
                    state.record_error();
                }
                let delay = retry_delay(schedule, "provider setup", &error);
                if !wait(delay, shutdown).await {
                    return None;
                }
            }
        }
    }
}

async fn wait(delay: Duration, shutdown: &mut shutdown::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = wait_for_shutdown(shutdown.clone()) => false,
    }
}

async fn wait_for_shutdown(mut shutdown: shutdown::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

fn retry_delay(
    schedule: &mut WatchSchedule,
    operation: &str,
    error: &dyn std::fmt::Display,
) -> Duration {
    let delay = schedule.failure_delay();
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    eprintln!(
        "{timestamp} {operation} failed (attempt {}): {error}; retrying in {delay:?}",
        schedule.consecutive_failures()
    );
    delay
}

fn should_clear_output(output: Format, stdout_is_terminal: bool) -> bool {
    output == Format::Text && stdout_is_terminal
}

fn print_report(
    report: &Report,
    options: &Options<'_>,
    clear_terminal: bool,
) -> std::io::Result<()> {
    let (rendered, newline) = match options.output {
        Format::Text => (text::render(report, options.render), false),
        Format::Json => (
            serde_json::to_string(&json::render(report, options.verbose))
                .map_err(std::io::Error::other)?,
            true,
        ),
    };
    let stdout = std::io::stdout();
    write_output(&mut stdout.lock(), &rendered, clear_terminal, newline)
}

fn write_output(
    writer: &mut impl Write,
    rendered: &str,
    clear_terminal: bool,
    newline: bool,
) -> std::io::Result<()> {
    if clear_terminal {
        writer.write_all(b"\x1b[2J\x1b[H")?;
    }
    writer.write_all(rendered.as_bytes())?;
    if newline {
        writer.write_all(b"\n")?;
    }
    writer.flush()
}

#[derive(Debug, Clone)]
struct WatchSchedule {
    interval: Duration,
    consecutive_failures: u32,
}

impl WatchSchedule {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            consecutive_failures: 0,
        }
    }

    fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    fn success_delay(&mut self) -> Duration {
        self.success_delay_with_entropy(time_entropy())
    }

    fn failure_delay(&mut self) -> Duration {
        self.failure_delay_with_entropy(time_entropy())
    }

    fn success_delay_with_entropy(&mut self, entropy: u64) -> Duration {
        self.consecutive_failures = 0;
        jitter_with_entropy(self.interval, entropy)
    }

    fn failure_delay_with_entropy(&mut self, entropy: u64) -> Duration {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        jitter_with_entropy(backoff_for(self.consecutive_failures), entropy).min(MAX_BACKOFF)
    }
}

fn time_entropy() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

fn backoff_for(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(31);
    INITIAL_BACKOFF
        .saturating_mul(1_u32 << exponent)
        .min(MAX_BACKOFF)
}

fn jitter_with_entropy(duration: Duration, entropy: u64) -> Duration {
    let millis = duration.as_millis().min(u64::MAX as u128) as u64;
    let spread = millis.saturating_mul(JITTER_PERCENT) / 100;
    if spread == 0 {
        return duration;
    }

    let width = spread.saturating_mul(2).saturating_add(1);
    let offset = entropy % width;
    Duration::from_millis(millis.saturating_sub(spread).saturating_add(offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn clears_only_interactive_text_output() {
        assert!(should_clear_output(Format::Text, true));
        assert!(!should_clear_output(Format::Text, false));
        assert!(!should_clear_output(Format::Json, true));
    }

    #[test]
    fn output_returns_broken_pipe_instead_of_panicking() {
        let error = write_output(&mut BrokenPipeWriter, "report", false, false).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn redirected_output_has_no_terminal_clear_sequence() {
        let mut output = Vec::new();
        write_output(&mut output, "report", false, true).unwrap();
        assert_eq!(output, b"report\n");
    }

    #[test]
    fn backoff_starts_at_one_second_and_caps_at_sixty() {
        let expected = [1, 2, 4, 8, 16, 32, 60, 60];
        for (failure, seconds) in (1..).zip(expected) {
            assert_eq!(backoff_for(failure), Duration::from_secs(seconds));
        }
    }

    #[test]
    fn backoff_does_not_overflow_for_large_failure_counts() {
        assert_eq!(backoff_for(u32::MAX), MAX_BACKOFF);

        let mut schedule = WatchSchedule::new(Duration::from_secs(5));
        schedule.consecutive_failures = u32::MAX;
        assert_eq!(schedule.failure_delay_with_entropy(u64::MAX), MAX_BACKOFF);
    }

    #[test]
    fn jitter_stays_within_twenty_percent() {
        let base = Duration::from_secs(10);
        for entropy in [0, 1, 2_000, u32::MAX as u64, u64::MAX] {
            let delay = jitter_with_entropy(base, entropy);
            assert!(delay >= Duration::from_secs(8));
            assert!(delay <= Duration::from_secs(12));
        }
    }

    #[test]
    fn success_resets_failure_sequence() {
        let mut schedule = WatchSchedule::new(Duration::from_secs(5));

        assert_eq!(
            schedule.failure_delay_with_entropy(200),
            Duration::from_secs(1)
        );
        assert_eq!(
            schedule.failure_delay_with_entropy(400),
            Duration::from_secs(2)
        );
        assert_eq!(schedule.consecutive_failures(), 2);

        assert_eq!(
            schedule.success_delay_with_entropy(1_000),
            Duration::from_secs(5)
        );
        assert_eq!(schedule.consecutive_failures(), 0);
        assert_eq!(
            schedule.failure_delay_with_entropy(200),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn sub_millisecond_delay_does_not_underflow() {
        let delay = Duration::from_micros(500);
        assert_eq!(jitter_with_entropy(delay, 0), delay);
    }
}
