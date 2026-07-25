//! Completed-mission landing-fuel metric and advisory policy.

use crate::domain::diagnostic::Diagnostic;
use crate::domain::quantity::QuantityOutput;
use crate::domain::warning::WarningCode;

const LOW_LANDING_FUEL_FRACTION: f64 = 0.05;

pub(super) struct LandingFuelOutcome {
    pub(super) value: Option<QuantityOutput>,
    pub(super) warning: Option<Diagnostic>,
}

pub(super) fn evaluate(
    completed: bool,
    remaining_fuel_kg: f64,
    maximum_fuel_kg: f64,
) -> LandingFuelOutcome {
    if !completed {
        return LandingFuelOutcome {
            value: None,
            warning: None,
        };
    }
    let threshold_kg = LOW_LANDING_FUEL_FRACTION * maximum_fuel_kg;
    let warning = (remaining_fuel_kg < threshold_kg).then(|| {
        Diagnostic::warning(
            WarningCode::LowLandingFuel,
            format!(
                "Landing fuel is {remaining_fuel_kg:.3} kg, below 5% of maximum fuel capacity ({threshold_kg:.3} kg)."
            ),
            "mission.landing_fuel",
        )
    });
    LandingFuelOutcome {
        value: Some(QuantityOutput::si(remaining_fuel_kg, "kg")),
        warning,
    }
}

#[cfg(test)]
mod tests;
