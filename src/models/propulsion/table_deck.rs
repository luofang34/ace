use serde_json::json;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::propulsion::{PropulsionMode, TableFuelValue, TablePropulsionDeck};
use crate::domain::warning::WarningCode;

#[derive(Debug)]
pub(super) struct TableDeckPoint {
    pub(super) thrust_per_engine_n: f64,
    pub(super) fuel: TableFuelValue,
    pub(super) extrapolated: bool,
    pub(super) warnings: Vec<Diagnostic>,
}

pub(super) fn evaluate(
    deck: &TablePropulsionDeck,
    mode: PropulsionMode,
    altitude_m: f64,
    mach: f64,
) -> AexResult<TableDeckPoint> {
    if deck.mode(mode).is_none() {
        return Err(AexError::validation(
            "MISSING_TABLE_MODE",
            "profile.parameters.table_deck.modes",
            format!("table deck does not contain mode {}", mode.wire_name()),
        ));
    }
    let interpolation = deck.interpolate(mode, altitude_m, mach).ok_or_else(|| {
        AexError::validation(
            "INVALID_TABLE_MATRIX_DIMENSIONS",
            "profile.parameters.table_deck.modes",
            "table matrices do not match their interpolation axes",
        )
    })?;
    validate_interpolated_values(interpolation.thrust_per_engine_n, interpolation.fuel)?;
    let mut warnings = Vec::new();
    add_extrapolation_warning(
        &mut warnings,
        interpolation.altitude_extrapolated,
        "altitude",
        altitude_m,
        "m",
        &deck.altitude_axis_m,
    );
    add_extrapolation_warning(
        &mut warnings,
        interpolation.mach_extrapolated,
        "mach",
        mach,
        "1",
        &deck.mach_axis,
    );
    Ok(TableDeckPoint {
        thrust_per_engine_n: interpolation.thrust_per_engine_n,
        fuel: interpolation.fuel,
        extrapolated: interpolation.altitude_extrapolated || interpolation.mach_extrapolated,
        warnings,
    })
}

fn validate_interpolated_values(thrust: f64, fuel: TableFuelValue) -> AexResult<()> {
    let fuel_value = fuel.value();
    if thrust.is_finite() && thrust > 0.0 && fuel_value.is_finite() && fuel_value > 0.0 {
        Ok(())
    } else {
        Err(AexError::analysis(
            "INVALID_TABLE_EXTRAPOLATION",
            "table interpolation produced a nonpositive or non-finite thrust or fuel value",
        ))
    }
}

fn add_extrapolation_warning(
    warnings: &mut Vec<Diagnostic>,
    extrapolated: bool,
    variable: &'static str,
    value: f64,
    unit: &'static str,
    axis: &[f64],
) {
    if !extrapolated {
        return;
    }
    warnings.push(Diagnostic::warning_with_context(
        WarningCode::ModelExtrapolation,
        format!(
            "Propulsion table extrapolated {variable} {value:.3} {unit} outside [{:.3}, {:.3}] {unit}.",
            axis[0],
            axis[axis.len() - 1]
        ),
        format!("aircraft.propulsion.profile.table_deck.axes.{variable}"),
        json!({
            "value": value,
            "minimum": axis[0],
            "maximum": axis[axis.len() - 1],
            "unit": unit,
            "basis": "tabulated_data",
        }),
    ));
}
