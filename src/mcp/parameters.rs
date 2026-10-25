use crate::domain::diagnostic::AexResult;

pub(super) fn parse_wing_loading(raw: &str) -> AexResult<f64> {
    crate::cli::commands::parse_wing_loading(raw)
}
