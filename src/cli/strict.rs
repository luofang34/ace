use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};

pub(super) fn enforce(strict: bool, warnings: &[Diagnostic]) -> AexResult<()> {
    let promoted = warnings.iter().find(|warning| {
        matches!(
            warning.code.as_str(),
            "MODEL_EXTRAPOLATION" | "AGENT_ASSUMPTION" | "PARAMETER_OUTSIDE_TYPICAL"
        )
    });
    if strict && let Some(warning) = promoted {
        return Err(AexError::analysis(
            "STRICT_WARNING_FAILURE",
            format!("{}: {}", warning.code, warning.message),
        ));
    }
    Ok(())
}
