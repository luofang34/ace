use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::schema::ResolvedScenario;
use crate::domain::warning::WarningCode;
use crate::models::mass_properties::longitudinal_stations;

/// Fraction of the weight the wing carries when the horizontal tail trims the
/// aircraft at `center_of_gravity_m`, by moments about the tail aerodynamic center.
pub(super) fn trim_wing_lift_fraction(
    scenario: &ResolvedScenario,
    center_of_gravity_m: f64,
) -> AexResult<f64> {
    let stations = longitudinal_stations(scenario).ok_or_else(|| {
        AexError::validation(
            "CG_TRIM_UNSUPPORTED",
            "condition.center_of_gravity",
            "a CG-dependent point needs a horizontal tail and fuselage geometry",
        )
    })?;
    wing_lift_fraction(
        center_of_gravity_m,
        stations.wing_aerodynamic_center_m,
        stations.tail_aerodynamic_center_m,
    )
}

/// Wing share of the weight from moments about the tail aerodynamic center, for
/// stations measured from one longitudinal datum.
pub(crate) fn wing_lift_fraction(
    center_of_gravity_m: f64,
    wing_aerodynamic_center_m: f64,
    tail_aerodynamic_center_m: f64,
) -> AexResult<f64> {
    let arm = tail_aerodynamic_center_m - wing_aerodynamic_center_m;
    let fraction = (tail_aerodynamic_center_m - center_of_gravity_m) / arm;
    if !(fraction.is_finite() && fraction > 0.0 && arm > 0.0) {
        return Err(AexError::validation(
            "CG_OUTSIDE_TRIM_RANGE",
            "condition.center_of_gravity",
            format!(
                "CG at {center_of_gravity_m:.3} m is not ahead of the tail aerodynamic center at {tail_aerodynamic_center_m:.3} m, or the tail is not behind the wing"
            ),
        ));
    }
    Ok(fraction)
}

/// CG-trimmed results scale with the CG-to-wing distance, so a wing station taken
/// from the layout correlation is reported rather than trusted silently.
pub(super) fn correlated_station_warning(scenario: &ResolvedScenario) -> Vec<Diagnostic> {
    match longitudinal_stations(scenario) {
        Some(stations) if !stations.wing_station_declared => {
            vec![Diagnostic::warning_with_context(
                WarningCode::CorrelatedGeometryStation,
                format!(
                    "CG trim uses a correlated wing aerodynamic center at {:.3} m; declare aircraft.geometry.wing.aerodynamic_center",
                    stations.wing_aerodynamic_center_m
                ),
                "aircraft.geometry.wing.aerodynamic_center",
                serde_json::json!({"estimated_m": stations.wing_aerodynamic_center_m}),
            )]
        }
        _ => Vec::new(),
    }
}
