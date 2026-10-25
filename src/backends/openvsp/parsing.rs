use crate::domain::diagnostic::{AexError, AexResult};

pub(super) fn marker_number(output: &str, marker: &str) -> AexResult<f64> {
    let raw = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(marker))
        .ok_or_else(|| parse_failure(format!("missing marker {marker}")))?;
    let value = raw
        .trim()
        .parse::<f64>()
        .map_err(|source| parse_failure(format!("invalid {marker} value: {source}")))?;
    if !value.is_finite() {
        return Err(parse_failure(format!(
            "{marker} must be finite, got {value}"
        )));
    }
    Ok(value)
}

pub(super) fn positive_marker_number(output: &str, marker: &str) -> AexResult<f64> {
    let value = marker_number(output, marker)?;
    if value <= 0.0 {
        return Err(parse_failure(format!(
            "{marker} must be positive, got {value}"
        )));
    }
    Ok(value)
}

fn parse_failure(message: impl Into<String>) -> AexError {
    AexError::BackendExecution {
        backend: "openvsp".to_owned(),
        operation: "result parsing".to_owned(),
        message: message.into(),
    }
}
