use crate::domain::diagnostic::AexError;

use super::persist_chart_blocking;

#[test]
fn artifact_request_fails_when_study_has_no_chart() {
    let result = persist_chart_blocking(None, Some("missing.svg"));

    assert!(matches!(
        result,
        Err(AexError::Validation {
            code: "STUDY_CHART_UNAVAILABLE",
            ..
        })
    ));
    assert!(persist_chart_blocking(None, None).is_ok());
}
