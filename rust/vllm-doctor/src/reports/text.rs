//! Plain-text report renderer: comfy-table for column layouts, hand-rolled panels.
use comfy_table::presets::NOTHING;
use comfy_table::{Cell, CellAlignment, Table};
use unicode_width::UnicodeWidthStr;

use crate::metrics::{all_specs, detect_replica_label};
use crate::models::{Finding, RuleResult, Severity};
use crate::reports::Report;
use crate::reports::format::format_value;

const BAR_WIDTH: usize = 20;
const BAR_FILLED: char = '█';
const BAR_EMPTY: char = '░';
const PANEL_OUTER_WIDTH: usize = 78;
// Each content row is "│  " + inner + "  │" — 6 framing chars around the text.
const PANEL_INNER_WIDTH: usize = PANEL_OUTER_WIDTH - 6;
const MAX_REPLICAS: usize = 6;

/// A borderless table that sizes columns to their content.
fn borderless_table() -> Table {
    let mut table = Table::new();
    table.load_preset(NOTHING);
    table
}

fn right(value: impl ToString) -> Cell {
    Cell::new(value).set_alignment(CellAlignment::Right)
}

/// Render a report as structured plain text.
pub fn render(report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    render_header(report, &mut out);
    render_findings(report, &mut out);
    render_check_list(report, &mut out);
    render_notices(report, &mut out);
    if verbose {
        render_metrics(report, &mut out);
        if let Some(label) = detect_replica_label(report.metric_series()) {
            render_replica_metrics(report, label, &mut out);
        }
    }
    out
}

fn render_notices(report: &Report, out: &mut String) {
    let notices = crate::reports::notices::resolve_notices(&report.diagnosis);
    if notices.is_empty() {
        return;
    }
    for notice in &notices {
        out.push_str(&format!("⚠ {notice}\n"));
    }
    out.push('\n');
}

fn render_header(report: &Report, out: &mut String) {
    let health_str = report.health().to_string().to_uppercase();
    let since = report.since();
    let width = PANEL_OUTER_WIDTH;

    let header_text = format!(
        "vLLM Doctor  ·  Health: {}  ·  Since: {}",
        health_str, since
    );
    let padded = pad_center(&header_text, width);

    out.push_str(&format!("──{}──\n", "─".repeat(width)));
    out.push_str(&format!("  {}\n", padded));
    out.push_str(&format!("──{}──\n", "─".repeat(width)));
    out.push('\n');
}

fn render_findings(report: &Report, out: &mut String) {
    let fired: Vec<(&RuleResult, &Finding)> = report
        .checks()
        .iter()
        .filter_map(|check| check.finding.as_ref().map(|f| (check, f)))
        .collect();

    if fired.is_empty() {
        out.push_str("No issues detected.\n\n");
        return;
    }

    for (_check, finding) in &fired {
        render_finding_panel(finding, out);
        out.push('\n');
    }
}

fn render_finding_panel(finding: &Finding, out: &mut String) {
    let icon = severity_icon(finding.severity);
    let title = format!(
        "{} {}  [{} confidence]",
        icon, finding.title, finding.confidence
    );

    let outer = PANEL_OUTER_WIDTH;
    let inner = PANEL_INNER_WIDTH;
    let top = format!("╭{}╮", "─".repeat(outer - 2));
    let bottom = format!("╰{}╯", "─".repeat(outer - 2));

    out.push_str(&top);
    out.push('\n');
    out.push_str(&format!("│  {}  │\n", pad_center(&title, inner)));
    out.push_str(&format!("│  {}  │\n", pad_right("", inner)));

    for line in &finding.evidence {
        for wrapped in textwrap::wrap(line, inner) {
            out.push_str(&format!("│  {}  │\n", pad_right(&wrapped, inner)));
        }
    }

    if !finding.recommendations.is_empty() {
        out.push_str(&format!("│  {}  │\n", pad_right("", inner)));
        // Hanging indent: "→ " on the first line, two spaces on continuations.
        let opts = textwrap::Options::new(inner)
            .initial_indent("→ ")
            .subsequent_indent("  ");
        for rec in &finding.recommendations {
            for wrapped in textwrap::wrap(rec, &opts) {
                out.push_str(&format!("│  {}  │\n", pad_right(&wrapped, inner)));
            }
        }
    }

    out.push_str(&bottom);
    out.push('\n');
}

fn severity_icon(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "✖",
        Severity::Warning => "⚠",
        Severity::Info => "ℹ",
    }
}

fn render_check_list(report: &Report, out: &mut String) {
    if report.checks().is_empty() {
        return;
    }

    let mut table = borderless_table();
    for check in report.checks() {
        if let Some(finding) = &check.finding {
            let status = format!("{} {}", severity_icon(finding.severity), finding.severity);
            table.add_row(vec![
                Cell::new(check.name),
                Cell::new(status),
                Cell::new(format!("[{}]", finding.confidence)),
            ]);
        } else {
            table.add_row(vec![
                Cell::new(check.name),
                Cell::new("✓ ok"),
                Cell::new(""),
            ]);
        }
    }
    out.push_str(&table.to_string());
    out.push_str("\n\n");
}

fn render_metrics(report: &Report, out: &mut String) {
    out.push_str("Observed Metrics:\n\n");

    let mut table = borderless_table();
    table.set_header(vec![Cell::new("Metric"), right("Value")]);
    for spec in all_specs() {
        let spec: &dyn crate::metrics::MetricSpec = spec.as_ref();
        let display = spec.display();
        let value_str = match spec.extract(report.metric_series()) {
            Some(v) if v.is_finite() => {
                if display.bar {
                    cache_bar(v)
                } else {
                    format_value(v, &display.fmt)
                }
            }
            _ => "n/a".to_string(),
        };
        table.add_row(vec![Cell::new(&display.title), right(value_str)]);
    }
    out.push_str(&table.to_string());
    out.push_str("\n\n");
}

fn render_replica_metrics(report: &Report, label: &str, out: &mut String) {
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

    let mut table = borderless_table();
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
                id: "queue_pressure",
                name: "Queue Pressure",
                title: "Queue pressure",
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
        let text = render(&report, false);

        assert!(text.contains("Health: WARNING"));
        assert!(text.contains("Queue Pressure"));
        assert!(text.contains("Waiting requests: 5"));
        assert!(text.contains("→ Add replicas"));
    }

    #[test]
    fn text_report_verbose_shows_metrics() {
        let report = Report::new(sample_result());
        let text = render(&report, true);
        assert!(text.contains("Observed Metrics"));
        assert!(text.contains("GPU Cache Usage"));
        assert!(text.contains("92%"));
    }

    #[test]
    fn text_report_shows_healthy_when_no_findings() {
        let mut result = sample_result();
        result.checks[0].finding = None;
        let report = Report::new(result);
        let text = render(&report, false);

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
        let text = render(&report, false);

        // The tail of the sentence must survive (no truncation), on a continuation line.
        assert!(text.contains("completion"));
        // Every rendered panel row stays within the panel width.
        for line in text.lines().filter(|l| l.starts_with('│')) {
            assert!(line.chars().count() <= PANEL_OUTER_WIDTH);
        }
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
        let text = render(&Report::new(result), true);

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
        let text = render(&report, false);
        assert!(text.contains("require Prometheus"));
    }
}
