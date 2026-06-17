//! Plain-text report renderer with hand-rolled tables and panels.
use crate::metrics::all_specs;
use crate::models::{Finding, RuleResult, Severity};
use crate::reports::Report;
use crate::reports::format::format_value;

const BAR_WIDTH: usize = 20;
const BAR_FILLED: char = '█';
const BAR_EMPTY: char = '░';
const PANEL_OUTER_WIDTH: usize = 78;
const PANEL_INNER_WIDTH: usize = PANEL_OUTER_WIDTH - 4;

/// Render a report as structured plain text.
pub fn render(report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    render_header(report, &mut out);
    render_findings(report, &mut out);
    render_check_list(report, &mut out);
    if verbose {
        render_metrics(report, &mut out);
    }
    out
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
        out.push_str(&format!("│  {}  │\n", pad_right(line, inner)));
    }

    if !finding.recommendations.is_empty() {
        out.push_str(&format!("│  {}  │\n", pad_right("", inner)));
        for rec in &finding.recommendations {
            let row = format!("→ {}", rec);
            out.push_str(&format!("│  {}  │\n", pad_right(&row, inner)));
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

    let name_width = report
        .checks()
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(10)
        .max(20);

    for check in report.checks() {
        if let Some(finding) = &check.finding {
            let status = format!("{} {}", severity_icon(finding.severity), finding.severity);
            out.push_str(&format!(
                "  {:<name_width$}  {:^12}  [{}]\n",
                check.name, status, finding.confidence
            ));
        } else {
            out.push_str(&format!("  {:<name_width$}  ✓ ok\n", check.name));
        }
    }
    out.push('\n');
}

fn render_metrics(report: &Report, out: &mut String) {
    out.push_str("Observed Metrics:\n\n");

    let name_width = all_specs()
        .iter()
        .map(|s| s.display().title.len())
        .max()
        .unwrap_or(20)
        .max(28);
    let value_width = 28;

    let header = format!("{:<name_width$}  {:>value_width$}", "Metric", "Value");
    let separator = "─".repeat(header.len());
    out.push_str(&header);
    out.push('\n');
    out.push_str(&separator);
    out.push('\n');

    for spec in all_specs() {
        let spec: &dyn crate::metrics::MetricSpec = spec.as_ref();
        let value: Option<f64> = spec.extract(report.metric_series());
        let display = spec.display();
        let value_str = match value {
            Some(v) if v.is_finite() => {
                if display.bar {
                    cache_bar(v).to_string()
                } else {
                    format_value(v, &display.fmt)
                }
            }
            _ => "n/a".to_string(),
        };
        out.push_str(&format!(
            "{:<name_width$}  {:>value_width$}\n",
            display.title, value_str
        ));
    }
    out.push('\n');
}

fn cache_bar(value: f64) -> String {
    let filled = (value * BAR_WIDTH as f64).round() as usize;
    let filled = filled.clamp(0, BAR_WIDTH);
    let bar: String = std::iter::repeat_n(BAR_FILLED, filled)
        .chain(std::iter::repeat_n(BAR_EMPTY, BAR_WIDTH - filled))
        .collect();
    format!("{} {}%", bar, (value * 100.0).round() as usize)
}

fn pad_right(s: &str, width: usize) -> String {
    let visible_len = s.chars().count();
    if visible_len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - visible_len))
    }
}

fn pad_center(s: &str, width: usize) -> String {
    let visible_len = s.chars().count();
    if visible_len >= width {
        return s.chars().take(width).collect();
    }
    let total_pad = width - visible_len;
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
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
}
