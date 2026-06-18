//! Shared formatting helpers for report renderers.

pub fn format_value(value: f64, fmt: &str) -> String {
    if let Some(rest) = fmt.strip_prefix(".") {
        if let Some(digits) = rest.strip_suffix("f") {
            if let Ok(prec) = digits.parse::<usize>() {
                return format!("{value:.prec$}");
            }
        }
        if let Some(digits) = rest.strip_suffix("%") {
            if let Ok(prec) = digits.parse::<usize>() {
                return format!("{:.1$}%", value * 100.0, prec);
            }
        }
    }
    format!("{value}")
}

#[cfg(test)]
mod tests {
    use super::format_value;

    #[test]
    fn fixed_precision() {
        assert_eq!(format_value(1.2345, ".2f"), "1.23");
        assert_eq!(format_value(7.0, ".0f"), "7");
    }

    #[test]
    fn percent_scales_by_100() {
        assert_eq!(format_value(0.85, ".0%"), "85%");
        assert_eq!(format_value(0.8523, ".1%"), "85.2%");
    }

    #[test]
    fn unknown_format_falls_back_to_default() {
        assert_eq!(format_value(3.5, ""), "3.5");
    }
}
