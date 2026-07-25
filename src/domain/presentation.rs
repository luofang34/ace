use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::{AexError, AexResult};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DisplayUnitSystem {
    #[default]
    Si,
    AviationUs,
}

impl DisplayUnitSystem {
    pub(crate) fn parse(value: &str, path: &str) -> AexResult<Self> {
        match value {
            "si" => Ok(Self::Si),
            "aviation_us" => Ok(Self::AviationUs),
            other => Err(AexError::validation(
                "UNSUPPORTED_UNIT_SYSTEM",
                path,
                format!("expected si or aviation_us, got {other}"),
            )),
        }
    }
}
