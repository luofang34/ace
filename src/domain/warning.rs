use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};

macro_rules! warning_registry {
    ($($variant:ident => ($wire:literal, $promotes:literal, $description:literal)),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub(crate) enum WarningCode {
            $($variant),+
        }

        impl WarningCode {
            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }

            pub(crate) fn from_wire(value: &str) -> Option<Self> {
                match value {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        const WARNING_POLICY: &[WarningPolicyEntry] = &[
            $(WarningPolicyEntry {
                code: WarningCode::$variant,
                promotes_in_strict: $promotes,
                description: $description,
            }),+
        ];
    };
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct WarningPolicyEntry {
    pub(crate) code: WarningCode,
    pub(crate) promotes_in_strict: bool,
    pub(crate) description: &'static str,
}

warning_registry! {
    AgentAssumption => (
        "AGENT_ASSUMPTION",
        true,
        "A value was supplied by an agent assumption rather than declared source data."
    ),
    ApproximateTakeoffDistance => (
        "APPROXIMATE_TAKEOFF_DISTANCE",
        false,
        "Takeoff distance uses the documented conceptual energy approximation."
    ),
    CertificationUseNotProhibited => (
        "CERTIFICATION_USE_NOT_PROHIBITED",
        false,
        "Example metadata does not explicitly prohibit certification use."
    ),
    CrossClassComparison => (
        "CROSS_CLASS_COMPARISON",
        false,
        "A comparison mixes propulsion classes with non-equivalent loading metrics."
    ),
    CruiseAltitudeLimitExceeded => (
        "CRUISE_ALTITUDE_LIMIT_EXCEEDED",
        false,
        "A declared cruise condition exceeds an aircraft operating limit."
    ),
    CruiseCapabilityUnavailable => (
        "CRUISE_CAPABILITY_UNAVAILABLE",
        false,
        "Installed capability cannot produce a supported achieved-cruise result."
    ),
    CruiseConditionUnsupported => (
        "CRUISE_CONDITION_UNSUPPORTED",
        false,
        "A declared cruise condition cannot be evaluated by the selected model."
    ),
    FuelCapacityExceeded => (
        "FUEL_CAPACITY_EXCEEDED",
        false,
        "The requested initial fuel load exceeds tank capacity."
    ),
    FuelExhausted => (
        "FUEL_EXHAUSTED",
        false,
        "Usable fuel is depleted before the mission completes."
    ),
    IndeterminateRequirement => (
        "INDETERMINATE_REQUIREMENT",
        false,
        "A boundary-limited requirement margin cannot be plotted as pass or fail."
    ),
    IrregularTradeSpace => (
        "IRREGULAR_TRADE_SPACE",
        false,
        "A nonrectangular study trade space remains a Pareto scatter without a fabricated surface."
    ),
    LowLandingFuel => (
        "LOW_LANDING_FUEL",
        false,
        "A completed mission lands with less than 5% of maximum fuel capacity."
    ),
    LowFidelityModel => (
        "LOW_FIDELITY_MODEL",
        false,
        "The result uses a documented conceptual-fidelity model."
    ),
    ModelExtrapolation => (
        "MODEL_EXTRAPOLATION",
        true,
        "A model is evaluated outside its calibrated or declared range."
    ),
    NativeStabilityNotModeled => (
        "NATIVE_STABILITY_NOT_MODELED",
        false,
        "The native backend does not estimate stability derivatives."
    ),
    ParameterOutsideTypical => (
        "PARAMETER_OUTSIDE_TYPICAL",
        true,
        "A resolved profile parameter is outside its registered typical range."
    ),
    SimplifiedPayloadRange => (
        "SIMPLIFIED_PAYLOAD_RANGE",
        false,
        "Payload-range values use a representative cruise approximation."
    ),
    StudyChartProjected => (
        "STUDY_CHART_PROJECTED",
        false,
        "A higher-dimensional study chart displays a declared two-axis projection."
    ),
    TransonicDragApproximation => (
        "TRANSONIC_DRAG_APPROXIMATION",
        false,
        "Wave drag uses the documented conceptual power-law correction."
    ),
    ZeroBreguetFuelBurn => (
        "ZERO_BREGUET_FUEL_BURN",
        false,
        "The Breguet estimate is zero because the simulated mission burns no fuel."
    ),
}

pub(crate) const fn warning_policy() -> &'static [WarningPolicyEntry] {
    WARNING_POLICY
}

pub(crate) fn enforce_strict(strict: bool, warnings: &[Diagnostic]) -> AexResult<()> {
    if !strict {
        return Ok(());
    }
    let promoted = warnings.iter().find(|warning| {
        WarningCode::from_wire(&warning.code).is_some_and(|code| {
            warning_policy()
                .iter()
                .any(|entry| entry.code == code && entry.promotes_in_strict)
        })
    });
    if let Some(warning) = promoted {
        Err(AexError::analysis(
            "STRICT_WARNING_FAILURE",
            format!("{}: {}", warning.code, warning.message),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn policy_markdown() -> String {
    let rows = warning_policy()
        .iter()
        .map(|entry| {
            format!(
                "| `{}` | {} | {} |\n",
                entry.code.as_str(),
                if entry.promotes_in_strict {
                    "yes"
                } else {
                    "no"
                },
                entry.description
            )
        })
        .collect::<String>();
    format!(
        "# Strict warning policy\n\n\
         Strict mode promotes only warnings marked `yes` to `STRICT_WARNING_FAILURE`.\n\
         The wire codes and decisions below are generated from the runtime registry.\n\n\
         | Warning code | Promoted | Meaning |\n\
         | --- | --- | --- |\n{rows}"
    )
}

#[cfg(test)]
mod tests;
