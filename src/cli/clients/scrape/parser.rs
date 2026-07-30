//! Prometheus text exposition format parser.
//!
//! This is a minimal, self-contained parser for the subset of the Prometheus
//! text exposition format that vLLM emits. It is not a full OpenMetrics parser:
//! it handles gauge/counter/untyped lines, optional labels, and optional
//! timestamps, and ignores `# HELP` and `# TYPE` metadata lines.
use std::collections::HashMap;

use regex::Regex;

const ESCAPED: [&str; 4] = ["\\n", "\\\"", "\\\\", "\\t"];
const UNESCAPED: [&str; 4] = ["\n", "\"", "\\", "\t"];

/// One parsed sample from a scrape.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrapeSample {
    pub metric: String,
    pub labels: HashMap<String, String>,
    pub value: f64,
    pub timestamp: Option<f64>,
}

/// Parse Prometheus text exposition into samples.
pub fn parse_scrape(input: &str) -> Vec<ScrapeSample> {
    input.lines().filter_map(parse_scrape_line).collect()
}

fn parse_scrape_line(line: &str) -> Option<ScrapeSample> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let name_end = line.find(['{', ' '])?;
    let metric = line[..name_end].trim().to_string();
    let rest = &line[name_end..];

    let (labels, after_labels) = if rest.starts_with('{') {
        let close = rest.find('}')?;
        let labels = parse_labels(&rest[1..close]);
        (labels, &rest[close + 1..])
    } else {
        (HashMap::new(), rest)
    };

    let mut tokens = after_labels.split_whitespace();
    let value_token = tokens.next()?;
    let value = parse_value(value_token)?;
    let timestamp = tokens.next().and_then(|t| t.parse().ok());

    Some(ScrapeSample {
        metric,
        labels,
        value,
        timestamp,
    })
}

fn parse_labels(input: &str) -> HashMap<String, String> {
    input
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim().to_string();
            let raw_value = parts.next()?.trim();
            let value = raw_value
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .map(unescape_label_value)
                .unwrap_or_else(|| raw_value.to_string());
            Some((key, value))
        })
        .collect()
}

fn unescape_label_value(value: &str) -> String {
    let mut result = value.to_string();
    for (escaped, unescaped) in ESCAPED.iter().zip(UNESCAPED.iter()) {
        result = result.replace(escaped, unescaped);
    }
    result
}

fn parse_value(token: &str) -> Option<f64> {
    match token {
        "NaN" => Some(f64::NAN),
        "+Inf" => Some(f64::INFINITY),
        "-Inf" => Some(f64::NEG_INFINITY),
        s => s.parse().ok(),
    }
}

/// Extract all metric names from the exposition text.
pub fn metric_names(input: &str) -> Vec<String> {
    let re =
        Regex::new(r"^(?P<name>[a-zA-Z_:][a-zA-Z0-9_:]*)(\{|\s)").expect("valid metric name regex");
    let mut names: Vec<String> = input
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
        .filter_map(|line| {
            re.captures(line.trim())
                .map(|caps| caps.name("name").unwrap().as_str().to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colon_metric_name() {
        let samples = parse_scrape("vllm:num_requests_running{model_name=\"llama\"} 10.0\n");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].metric, "vllm:num_requests_running");
        assert_eq!(samples[0].labels["model_name"], "llama");
        assert!((samples[0].value - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_metric_without_labels() {
        let samples = parse_scrape("node_cpu_seconds_total 42.0\n");
        assert_eq!(samples.len(), 1);
        assert!(samples[0].labels.is_empty());
        assert!((samples[0].value - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_metric_with_timestamp() {
        let samples = parse_scrape("node_cpu_seconds_total 42.0 1234567890\n");
        assert_eq!(samples[0].timestamp, Some(1234567890.0));
    }

    #[test]
    fn parse_nan_inf_values() {
        let samples = parse_scrape("m NaN\nn +Inf\no -Inf\n");
        assert!(samples[0].value.is_nan());
        assert_eq!(samples[1].value, f64::INFINITY);
        assert_eq!(samples[2].value, f64::NEG_INFINITY);
    }

    #[test]
    fn parse_ignores_help_and_type_lines() {
        let samples = parse_scrape("# HELP m Metric\n# TYPE m gauge\nm 1.0\n");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].metric, "m");
    }

    #[test]
    fn parse_unescapes_label_values() {
        let samples = parse_scrape("path{value=\"C:\\\\DIR\\\\FILE.TXT\"} 1.0");
        assert_eq!(samples[0].labels["value"], r#"C:\DIR\FILE.TXT"#);
    }

    #[test]
    fn metric_names_returns_unique_sorted_names() {
        let names = metric_names("a 1\nb 2\na 3\n# comment\n");
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_empty_and_comments_returns_empty() {
        assert!(parse_scrape("").is_empty());
        assert!(parse_scrape("# comment\n").is_empty());
        assert!(parse_scrape("  \n").is_empty());
    }
}
