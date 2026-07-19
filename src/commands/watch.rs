//! Continuous diagnosis orchestration for the CLI.
use std::io::{IsTerminal, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vllm_doctor::cli::Format;
use vllm_doctor::models::DiagnosisResult;
use vllm_doctor::reports::{RenderOptions, Report, json, text};
use vllm_doctor::runner::{DiagnoseRequest, DiagnoseRunner, RunnerError, transition_label};
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const JITTER_PERCENT: u64 = 20;

pub(super) struct Options<'a> {
    pub(super) output: Format,
    pub(super) verbose: bool,
    pub(super) save: bool,
    pub(super) render: &'a RenderOptions,
}

pub(super) async fn run(request: DiagnoseRequest, options: Options<'_>) {
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
    let Some(runner) = connect(&request, &mut schedule).await else {
        return;
    };
    let clear_terminal = should_clear_output(options.output, std::io::stdout().is_terminal());
    let mut previous: Option<DiagnosisResult> = None;

    loop {
        let attempt = tokio::select! {
            result = runner.run_once() => result,
            _ = tokio::signal::ctrl_c() => break,
        };

        let delay = match attempt {
            Ok(result) => {
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

                previous = Some(result);
                schedule.success_delay()
            }
            Err(RunnerError::Fetch(error)) => retry_delay(&mut schedule, "collection", &error),
        };

        if !wait(delay).await {
            break;
        }
    }
}

async fn connect(
    request: &DiagnoseRequest,
    schedule: &mut WatchSchedule,
) -> Option<DiagnoseRunner> {
    loop {
        let attempt = tokio::select! {
            result = DiagnoseRunner::new(request.clone()) => result,
            _ = tokio::signal::ctrl_c() => return None,
        };

        match attempt {
            Ok(runner) => return Some(runner),
            Err(RunnerError::Fetch(error)) => {
                let delay = retry_delay(schedule, "provider setup", &error);
                if !wait(delay).await {
                    return None;
                }
            }
        }
    }
}

async fn wait(delay: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = tokio::signal::ctrl_c() => false,
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
