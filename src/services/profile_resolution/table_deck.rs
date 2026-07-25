use std::collections::BTreeMap;

use serde::Deserialize;
use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::propulsion::{
    PropulsionMode, TableFuelSchedule, TablePropulsionDeck, TablePropulsionMode,
};
use crate::domain::quantity::{Dimension, parse_quantity};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTableDeck {
    axes: RawTableAxes,
    modes: BTreeMap<String, RawTableMode>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTableAxes {
    mach: Vec<f64>,
    altitude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTableMode {
    thrust: RawTableMatrix,
    #[serde(default)]
    tsfc: Option<RawTableMatrix>,
    #[serde(default)]
    specific_impulse: Option<RawTableMatrix>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTableMatrix {
    unit: String,
    values: Vec<Vec<f64>>,
}

pub(super) fn parse(parameters: &Value) -> AexResult<Option<TablePropulsionDeck>> {
    let Some(value) = parameters.get("table_deck") else {
        return Ok(None);
    };
    let raw: RawTableDeck = serde_yaml::from_value(value.clone()).map_err(|source| {
        AexError::validation(
            "INVALID_TABLE_DECK",
            "profile.parameters.table_deck",
            source.to_string(),
        )
    })?;
    let mach_axis = validate_mach_axis(raw.axes.mach)?;
    let altitude_axis_m = parse_altitude_axis(raw.axes.altitude)?;
    let modes = parse_modes(raw.modes, altitude_axis_m.len(), mach_axis.len())?;
    Ok(Some(TablePropulsionDeck {
        mach_axis,
        altitude_axis_m,
        modes,
    }))
}

fn validate_mach_axis(values: Vec<f64>) -> AexResult<Vec<f64>> {
    validate_axis(
        &values,
        "profile.parameters.table_deck.axes.mach",
        |value| value >= 0.0,
        "finite, nonnegative",
    )?;
    Ok(values)
}

fn parse_altitude_axis(values: Vec<String>) -> AexResult<Vec<f64>> {
    let parsed = values
        .iter()
        .map(|value| parse_quantity(value, Dimension::Length))
        .collect::<AexResult<Vec<_>>>()?;
    validate_axis(
        &parsed,
        "profile.parameters.table_deck.axes.altitude",
        |_| true,
        "finite",
    )?;
    Ok(parsed)
}

fn validate_axis(
    values: &[f64],
    path: &'static str,
    value_is_valid: impl Fn(f64) -> bool,
    requirement: &'static str,
) -> AexResult<()> {
    if values.len() < 2
        || values
            .iter()
            .any(|value| !value.is_finite() || !value_is_valid(*value))
    {
        return Err(AexError::validation(
            "INVALID_TABLE_AXIS",
            path,
            format!("table axes require at least two {requirement} values"),
        ));
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(AexError::validation(
            "UNSORTED_TABLE_AXIS",
            path,
            "table axes must be strictly increasing",
        ));
    }
    Ok(())
}

fn parse_modes(
    mut raw_modes: BTreeMap<String, RawTableMode>,
    rows: usize,
    columns: usize,
) -> AexResult<Vec<TablePropulsionMode>> {
    for name in raw_modes.keys() {
        if parse_mode(name).is_none() {
            return Err(AexError::validation(
                "UNSUPPORTED_PROPULSION_MODE",
                format!("profile.parameters.table_deck.modes.{name}"),
                format!("unsupported propulsion mode {name}"),
            ));
        }
    }
    PropulsionMode::ALL
        .into_iter()
        .map(|mode| {
            let name = mode.wire_name();
            let raw = raw_modes.remove(name).ok_or_else(|| {
                AexError::validation(
                    "MISSING_TABLE_MODE",
                    "profile.parameters.table_deck.modes",
                    format!("table deck requires mode {name}"),
                )
            })?;
            parse_mode_data(mode, raw, rows, columns)
        })
        .collect()
}

fn parse_mode(name: &str) -> Option<PropulsionMode> {
    PropulsionMode::ALL
        .into_iter()
        .find(|mode| mode.wire_name() == name)
}

fn parse_mode_data(
    mode: PropulsionMode,
    raw: RawTableMode,
    rows: usize,
    columns: usize,
) -> AexResult<TablePropulsionMode> {
    let path = format!("profile.parameters.table_deck.modes.{}", mode.wire_name());
    validate_matrix(&raw.thrust, "N", &format!("{path}.thrust"), rows, columns)?;
    let fuel = match (raw.tsfc, raw.specific_impulse) {
        (Some(matrix), None) => {
            validate_matrix(&matrix, "kg/N/hr", &format!("{path}.tsfc"), rows, columns)?;
            TableFuelSchedule::TsfcKgNHr(matrix.values)
        }
        (None, Some(matrix)) => {
            validate_matrix(
                &matrix,
                "s",
                &format!("{path}.specific_impulse"),
                rows,
                columns,
            )?;
            TableFuelSchedule::SpecificImpulseS(matrix.values)
        }
        _ => {
            return Err(AexError::validation(
                "TABLE_FUEL_FIELD_EXCLUSIVITY",
                path,
                "exactly one of tsfc or specific_impulse is required",
            ));
        }
    };
    Ok(TablePropulsionMode {
        mode,
        thrust_n: raw.thrust.values,
        fuel,
    })
}

fn validate_matrix(
    matrix: &RawTableMatrix,
    expected_unit: &'static str,
    path: &str,
    rows: usize,
    columns: usize,
) -> AexResult<()> {
    if matrix.unit != expected_unit {
        return Err(AexError::validation(
            "INCOMPATIBLE_UNITS",
            path,
            format!("expected {expected_unit}"),
        ));
    }
    if matrix.values.len() != rows || matrix.values.iter().any(|row| row.len() != columns) {
        return Err(AexError::validation(
            "INVALID_TABLE_MATRIX_DIMENSIONS",
            path,
            format!("expected {rows} rows by {columns} columns"),
        ));
    }
    if matrix
        .values
        .iter()
        .flatten()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(AexError::validation(
            "INVALID_TABLE_CELL",
            path,
            "table cells must be finite and positive",
        ));
    }
    Ok(())
}
