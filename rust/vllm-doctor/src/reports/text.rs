//! Plain-text report renderer: comfy-table for column layouts, hand-rolled panels.
use comfy_table::presets::NOTHING;
use comfy_table::{Cell, CellAlignment, Table};
use owo_colors::{AnsiColors, OwoColorize};
use unicode_width::UnicodeWidthStr;

use crate::metrics::{all_specs, detect_replica_label};
use crate::models::{Finding, Health, Severity};
use crate::reports::format::format_value;
use crate::reports::{RenderOptions, Report};

const BAR_WIDTH: usize = 20;
const BAR_FILLED: char = '█';
const BAR_EMPTY: char = '░';
const MIN_WIDTH: usize = 60;
const MAX_WIDTH: usize = 120;
// A content row is "│  " + inner + "  │" — 6 framing chars around the text.
const PANEL_FRAME: usize = 6;
const MAX_REPLICAS: usize = 6;

/// Render a report as structured plain text.
pub fn render(report: &Report, opts: &RenderOptions) -> String {
    let outer = opts.width.clamp(MIN_WIDTH, MAX_WIDTH);
    let inner = outer - PANEL_FRAME;

    let mut out = String::new();
    render_header(report, opts, outer, &mut out);
    render_findings(report, opts, outer, inner, &mut out);
    render_check_list(report, opts, &mut out);
    render_notices(report, opts, &mut out);
    if opts.verbose {
        render_metrics(report, opts, &mut out);
        if let Some(label) = detect_replica_label(report.metric_series()) {
            render_replica_metrics(report, opts, label, &mut out);
        }
    }
    out
}

fn render_header(report: &Report, opts: &RenderOptions, outer: usize, out: &mut String) {
    let health = report.health();
    let label = health.to_string().to_uppercase();
    let since = report.since();

    // Pad from the plain text so an embedded color code doesn't skew centering.
    let plain = format!("vLLM Doctor  ·  Health: {label}  ·  Since: {since}");
    let pad = outer.saturating_sub(plain.width());
    let left = pad / 2;
    let right = pad - left;
    let health_text = paint(&label, health_color(health), true, opts.color);
    let line = format!("vLLM Doctor  ·  Health: {health_text}  ·  Since: {since}");

    let rule = "─".repeat(outer);
    out.push_str(&rule);
    out.push('\n');
    out.push_str(&" ".repeat(left));
    out.push_str(&line);
    out.push_str(&" ".repeat(right));
    out.push('\n');
    out.push_str(&rule);
    out.push_str("\n\n");
}

fn render_findings(
    report: &Report,
    opts: &RenderOptions,
    outer: usize,
    inner: usize,
    out: &mut String,
) {
    let fired: Vec<&Finding> = report
        .checks()
        .iter()
        .filter_map(|check| check.finding.as_ref())
        .collect();

    if fired.is_empty() {
        out.push_str("No issues detected.\n\n");
        return;
    }

    for finding in fired {
        render_finding_panel(finding, opts, outer, inner, out);
        out.push('\n');
    }
}

fn render_finding_panel(
    finding: &Finding,
    opts: &RenderOptions,
    outer: usize,
    inner: usize,
    out: &mut String,
) {
    let color = severity_color(finding.severity);
    let icon = severity_icon(finding.severity);
    let title = format!(
        "{} {}  [{} confidence]",
        icon, finding.title, finding.confidence
    );

    let bar = paint("│", color, false, opts.color);
    let content = |text: &str| format!("{bar}  {text}  {bar}\n");

    out.push_str(&paint(
        &format!("╭{}╮", "─".repeat(outer - 2)),
        color,
        false,
        opts.color,
    ));
    out.push('\n');
    let title_cell = paint(&pad_center(&title, inner), color, true, opts.color);
    out.push_str(&content(&title_cell));

    for line in &finding.evidence {
        for wrapped in textwrap::wrap(line, inner) {
            out.push_str(&content(&pad_right(&wrapped, inner)));
        }
    }

    if !finding.recommendations.is_empty() {
        out.push_str(&content(&pad_right("", inner)));
        // Hanging indent: "→ " on the first line, two spaces on continuations.
        let wrap_opts = textwrap::Options::new(inner)
            .initial_indent("→ ")
            .subsequent_indent("  ");
        for rec in &finding.recommendations {
            for wrapped in textwrap::wrap(rec, &wrap_opts) {
                // Tint the arrow to match the finding's severity color.
                let padded = pad_right(&wrapped, inner);
                let line = if opts.color {
                    padded.replacen('→', &paint("→", color, false, true), 1)
                } else {
                    padded
                };
                out.push_str(&content(&line));
            }
        }
    }

    out.push_str(&paint(
        &format!("╰{}╯", "─".repeat(outer - 2)),
        color,
        false,
        opts.color,
    ));
    out.push('\n');
}

fn render_check_list(report: &Report, opts: &RenderOptions, out: &mut String) {
    if report.checks().is_empty() {
        return;
    }

    let mut table = colored_table(opts);
    for check in report.checks() {
        if let Some(finding) = &check.finding {
            let status = Cell::new(format!(
                "{} {}",
                severity_icon(finding.severity),
                finding.severity
            ));
            table.add_row(vec![
                Cell::new(&check.name),
                paint_cell(status, finding.severity, opts),
                Cell::new(format!("[{}]", finding.confidence)),
            ]);
        } else {
            let mut ok = Cell::new("✓ ok");
            if opts.color {
                ok = ok.fg(comfy_table::Color::Green);
            }
            table.add_row(vec![Cell::new(&check.name), ok, Cell::new("")]);
        }
    }
    out.push_str(&table.to_string());
    out.push_str("\n\n");
}

fn render_notices(report: &Report, opts: &RenderOptions, out: &mut String) {
    let notices = crate::reports::notices::resolve_notices(&report.diagnosis);
    if notices.is_empty() {
        return;
    }
    for notice in &notices {
        let line = format!("⚠ {notice}");
        out.push_str(&paint(&line, AnsiColors::Yellow, false, opts.color));
        out.push('\n');
    }
    out.push('\n');
}

fn render_metrics(report: &Report, opts: &RenderOptions, out: &mut String) {
    out.push_str("Observed Metrics:\n\n");

    let mut table = colored_table(opts);
    table.set_header(vec![Cell::new("Metric"), right("Value")]);
    for spec in all_specs() {
        let spec: &dyn crate::metrics::MetricSpec = spec.as_ref();
        let display = spec.display();
        let value = spec.extract(report.metric_series());
        let value_str = match value {
            Some(v) if v.is_finite() => {
                if display.bar {
                    cache_bar(v)
                } else {
                    format_value(v, &display.fmt)
                }
            }
            _ => "n/a".to_string(),
        };
        let mut value_cell = right(value_str);
        if opts.color && display.bar {
            if let Some(v) = value {
                value_cell = value_cell.fg(bar_color(v));
            }
        }
        table.add_row(vec![Cell::new(&display.title), value_cell]);
    }
    out.push_str(&table.to_string());
    out.push_str("\n\n");
}

fn render_replica_metrics(report: &Report, opts: &RenderOptions, label: &str, out: &mut String) {
    use std::collections::{HashMap, HashSet};

    let snapshot = report.metric_series();

    // Collect per-replica values for each spec that has a breakdown.
    let mut specs_with_data: Vec<&dyn crate::metrics::MetricSpec> = Vec::new();
    let mut values: HashMap<&str, HashMap<String, Option<f64>>> = HashMap::new();
    for spec in all_specs() {
        let spec: &dyn crate::metrics::MetricSpec = spec.as_ref();
        if let Some(series) = spec.series(snapshot) {
            let breakdown = series.by(label);
            if !breakdown.is_empty() {
                values.insert(spec.output(), breakdown);
                specs_with_data.push(spec);
            }
        }
    }
    if specs_with_data.is_empty() {
        return;
    }

    let cell = |output: &str, replica: &str| -> Option<f64> {
        values
            .get(output)
            .and_then(|m| m.get(replica))
            .copied()
            .flatten()
    };

    // Order replicas by pressure: waiting, then cache usage, then running — descending.
    let mut replicas: Vec<String> = values
        .values()
        .flat_map(|m| m.keys().cloned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let sort_key = |r: &str| {
        (
            cell("num_requests_waiting", r).unwrap_or(0.0),
            cell("kv_cache_usage_perc", r).unwrap_or(0.0),
            cell("num_requests_running", r).unwrap_or(0.0),
        )
    };
    replicas.sort_by(|a, b| {
        sort_key(b)
            .partial_cmp(&sort_key(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let hidden = replicas.len().saturating_sub(MAX_REPLICAS);
    let visible: Vec<String> = replicas.into_iter().take(MAX_REPLICAS).collect();

    let fmt_cell = |spec: &dyn crate::metrics::MetricSpec, replica: &str| -> String {
        match cell(spec.output(), replica) {
            Some(v) if v.is_finite() => format_value(v, &spec.display().fmt),
            _ => "n/a".to_string(),
        }
    };

    let mut table = colored_table(opts);
    let mut header = vec![Cell::new("")];
    header.extend(visible.iter().map(right));
    if hidden > 0 {
        header.push(right(format!("+{hidden} more")));
    }
    table.set_header(header);

    for spec in &specs_with_data {
        let mut row = vec![Cell::new(spec.display().title.as_str())];
        row.extend(
            visible
                .iter()
                .map(|replica| right(fmt_cell(*spec, replica))),
        );
        if hidden > 0 {
            row.push(Cell::new(""));
        }
        table.add_row(row);
    }

    out.push_str(&format!("Observed Metrics per {label}:\n\n"));
    out.push_str(&table.to_string());
    out.push_str("\n\n");
}

/// Wrap `text` in ANSI color when `enabled`; otherwise return it unchanged so
/// piped/captured output stays plain.
fn paint(text: &str, color: AnsiColors, bold: bool, enabled: bool) -> String {
    if !enabled {
        return text.to_string();
    }
    if bold {
        text.color(color).bold().to_string()
    } else {
        text.color(color).to_string()
    }
}

fn severity_icon(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "✖",
        Severity::Warning => "⚠",
        Severity::Info => "ℹ",
    }
}

fn severity_color(severity: Severity) -> AnsiColors {
    match severity {
        Severity::Critical => AnsiColors::Red,
        Severity::Warning => AnsiColors::Yellow,
        Severity::Info => AnsiColors::Blue,
    }
}

fn health_color(health: Health) -> AnsiColors {
    match health {
        Health::Ok => AnsiColors::Green,
        Health::Info => AnsiColors::Blue,
        Health::Warning => AnsiColors::Yellow,
        Health::Critical => AnsiColors::Red,
    }
}

fn bar_color(value: f64) -> comfy_table::Color {
    if value >= 0.9 {
        comfy_table::Color::Red
    } else if value >= 0.7 {
        comfy_table::Color::Yellow
    } else {
        comfy_table::Color::Green
    }
}

fn comfy_severity(severity: Severity) -> comfy_table::Color {
    match severity {
        Severity::Critical => comfy_table::Color::Red,
        Severity::Warning => comfy_table::Color::Yellow,
        Severity::Info => comfy_table::Color::Blue,
    }
}

fn paint_cell(cell: Cell, severity: Severity, opts: &RenderOptions) -> Cell {
    if opts.color {
        cell.fg(comfy_severity(severity))
    } else {
        cell
    }
}

/// A borderless table that sizes columns to their content, with styling forced
/// on when color is requested (otherwise comfy-table may strip it off a pipe).
fn colored_table(opts: &RenderOptions) -> Table {
    let mut table = Table::new();
    table.load_preset(NOTHING);
    if opts.color {
        table.enforce_styling();
    }
    table
}

fn right(value: impl ToString) -> Cell {
    Cell::new(value).set_alignment(CellAlignment::Right)
}

fn cache_bar(value: f64) -> String {
    let filled = (value * BAR_WIDTH as f64).round() as usize;
    let filled = filled.clamp(0, BAR_WIDTH);
    let bar: String = std::iter::repeat_n(BAR_FILLED, filled)
        .chain(std::iter::repeat_n(BAR_EMPTY, BAR_WIDTH - filled))
        .collect();
    format!("{} {}%", bar, (value * 100.0).round() as usize)
}

/// Pad `s` on the right to a given terminal-column width. Uses display width so
/// wide glyphs (icons, box drawing) don't throw off panel borders.
fn pad_right(s: &str, width: usize) -> String {
    let visible = s.width();
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Center `s` within a given terminal-column width.
fn pad_center(s: &str, width: usize) -> String {
    let visible = s.width();
    if visible >= width {
        return s.to_string();
    }
    let total_pad = width - visible;
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    format!("{}{}{}", " ".repeat(left_pad), s, " ".repeat(right_pad))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::RuleResult;
    use crate::models::{Confidence, DiagnosisContext, DiagnosisResult, Severity};

    fn verbose() -> RenderOptions {
        RenderOptions {
            verbose: true,
            ..Default::default()
        }
    }

    fn sample_finding() -> Finding {
        Finding {
            severity: Severity::Warning,
            confidence: Confidence::Medium,
            title: "Queue Pressure".to_string(),
            summary: "5 requests are waiting in the queue".to_string(),
            signals: vec!["num_requests_waiting".to_string()],
            evidence: vec!["Waiting requests: 5".to_string()],
            likely_causes: vec!["Insufficient capacity".to_string()],
            recommendations: vec!["Add replicas".to_string()],
            related_metrics: vec!["vllm:num_requests_waiting".to_string()],
        }
    }

    fn sample_result() -> DiagnosisResult {
        DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            checks: vec![RuleResult {
                id: "queue_pressure".into(),
                name: "Queue Pressure".into(),
                title: "Queue pressure".into(),
                severity: Severity::Warning,
                finding: Some(sample_finding()),
            }],
            metric_series: MetricSeriesSnapshot {
                num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
                kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.92)]),
                ..Default::default()
            },
        }
    }

    #[test]
    fn text_report_includes_health_and_finding() {
        let report = Report::new(sample_result());
        let text = render(&report, &RenderOptions::default());

        assert!(text.contains("Health: WARNING"));
        assert!(text.contains("Queue Pressure"));
        assert!(text.contains("Waiting requests: 5"));
        assert!(text.contains("→ Add replicas"));
    }

    #[test]
    fn text_report_verbose_shows_metrics() {
        let report = Report::new(sample_result());
        let text = render(&report, &verbose());
        assert!(text.contains("Observed Metrics"));
        assert!(text.contains("GPU Cache Usage"));
        assert!(text.contains("92%"));
    }

    #[test]
    fn text_report_shows_healthy_when_no_findings() {
        let mut result = sample_result();
        result.checks[0].finding = None;
        let report = Report::new(result);
        let text = render(&report, &RenderOptions::default());

        assert!(text.contains("Health: HEALTHY"));
        assert!(text.contains("No issues detected"));
        assert!(text.contains("✓ ok"));
    }

    #[test]
    fn long_recommendation_wraps_instead_of_truncating() {
        let mut result = sample_result();
        let long = "Check client timeout settings relative to the observed TTFT and TPOT \
            latencies and increase them where requests are being aborted before completion";
        result.checks[0].finding.as_mut().unwrap().recommendations = vec![long.to_string()];
        let report = Report::new(result);
        let text = render(&report, &RenderOptions::default());

        // The tail of the sentence must survive (no truncation), on a continuation line.
        assert!(text.contains("completion"));
        // Every rendered panel row stays within the default panel width.
        for line in text.lines().filter(|l| l.starts_with('│')) {
            assert!(line.chars().count() <= RenderOptions::default().width);
        }
    }

    #[test]
    fn width_controls_panel_size() {
        let report = Report::new(sample_result());
        let text = render(
            &report,
            &RenderOptions {
                width: 100,
                ..Default::default()
            },
        );
        assert!(
            text.lines()
                .any(|l| l.starts_with('╭') && l.chars().count() == 100)
        );
    }

    #[test]
    fn color_is_emitted_only_when_enabled() {
        let report = Report::new(sample_result());
        let plain = render(&report, &RenderOptions::default());
        let colored = render(
            &report,
            &RenderOptions {
                color: true,
                ..Default::default()
            },
        );
        assert!(!plain.contains('\u{1b}'));
        assert!(colored.contains('\u{1b}'));
    }

    #[test]
    fn verbose_renders_per_replica_table() {
        let mut result = sample_result();
        result.metric_series = MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![
                MetricSample::new(8.0).with_label("pod", "pod-a"),
                MetricSample::new(2.0).with_label("pod", "pod-b"),
            ]),
            num_requests_running: MetricSeries::from_samples(vec![
                MetricSample::new(1.0).with_label("pod", "pod-a"),
                MetricSample::new(3.0).with_label("pod", "pod-b"),
            ]),
            ..Default::default()
        };
        let text = render(&Report::new(result), &verbose());

        assert!(text.contains("Observed Metrics per pod"));
        assert!(text.contains("pod-a"));
        assert!(text.contains("pod-b"));
        // Most-pressured replica (pod-a, 8 waiting) sorts before pod-b.
        let pos = text.find("Observed Metrics per pod").unwrap();
        let table = &text[pos..];
        assert!(table.find("pod-a").unwrap() < table.find("pod-b").unwrap());
    }

    #[test]
    fn scrape_mode_renders_notice() {
        let mut result = sample_result();
        result.context = result
            .context
            .with_client_mode(crate::models::ClientMode::Scrape);
        let report = Report::new(result);
        let text = render(&report, &RenderOptions::default());
        assert!(text.contains("require Prometheus"));
    }
}
