//! Shared formatting helpers for report renderers.

pub fn format_value(value: f64, fmt: &str) -> String {
    if let Some(n) = fmt.strip_prefix(".") {
        if let Some(digits) = n.strip_suffix("f") {
            if let Ok(prec) = digits.parse::<usize>() {
                return format!("{:.1$}", value, prec);
            }
        }
    }
    format!("{}", value)
}
