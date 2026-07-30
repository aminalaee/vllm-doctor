use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid observation window: {0}")]
pub struct WindowParseError(String);

/// Parse a diagnosis `since` string into an observation window in seconds.
pub fn parse_window_seconds(since: &str) -> Result<u64, WindowParseError> {
    let trimmed = since.trim();
    if trimmed.is_empty() {
        return Err(WindowParseError("window string is empty".to_string()));
    }
    if trimmed.eq_ignore_ascii_case("now") {
        return Ok(300);
    }

    let (number, multiplier) = match trimmed.as_bytes().last() {
        Some(b's') => (&trimmed[..trimmed.len() - 1], 1),
        Some(b'm') => (&trimmed[..trimmed.len() - 1], 60),
        Some(b'h') => (&trimmed[..trimmed.len() - 1], 3_600),
        Some(b'd') => (&trimmed[..trimmed.len() - 1], 86_400),
        _ => {
            return Err(WindowParseError(format!(
                "unsupported window unit in `{since}`"
            )));
        }
    };
    let value: u64 = number
        .parse()
        .map_err(|_| WindowParseError(format!("malformed window duration `{since}`")))?;
    if value == 0 {
        return Err(WindowParseError(
            "window duration must be greater than zero".to_string(),
        ));
    }
    value
        .checked_mul(multiplier)
        .ok_or_else(|| WindowParseError(format!("window `{since}` overflows")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_windows() {
        assert_eq!(parse_window_seconds("now").unwrap(), 300);
        assert_eq!(parse_window_seconds("30s").unwrap(), 30);
        assert_eq!(parse_window_seconds("5m").unwrap(), 300);
        assert_eq!(parse_window_seconds("2h").unwrap(), 7_200);
        assert_eq!(parse_window_seconds("1d").unwrap(), 86_400);
    }

    #[test]
    fn rejects_invalid_windows() {
        for value in ["", "0s", "-5m", "abc", "1h30m", "999999999999999999999d"] {
            assert!(parse_window_seconds(value).is_err(), "accepted `{value}`");
        }
    }
}
