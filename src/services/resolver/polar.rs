use crate::domain::aerodynamics::PolarTable;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::WaveDrag;

pub(super) fn resolve(
    table: Option<PolarTable>,
    wave_drag: Option<&WaveDrag>,
    configuration: &str,
) -> AexResult<Option<PolarTable>> {
    let Some(table) = table else {
        return Ok(None);
    };
    let root = format!("aircraft.aerodynamics.{configuration}.polar_table");
    if wave_drag.is_some() {
        return Err(AexError::validation(
            "POLAR_MODEL_CONFLICT",
            &root,
            "polar_table and wave_drag cannot be combined",
        ));
    }
    validate_axis(&table.mach, &format!("{root}.mach"))?;
    validate_values(&table.cd0, table.mach.len(), &format!("{root}.cd0"))?;
    validate_values(&table.cl_max, table.mach.len(), &format!("{root}.cl_max"))?;
    validate_induced_schedule(&table, &root)?;
    Ok(Some(table))
}

fn validate_axis(values: &[f64], path: &str) -> AexResult<()> {
    if values.len() < 2
        || values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(AexError::validation(
            "INVALID_POLAR_TABLE_AXIS",
            path,
            "Mach axis requires at least two finite nonnegative values",
        ));
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(AexError::validation(
            "UNSORTED_POLAR_TABLE_AXIS",
            path,
            "Mach axis must be strictly increasing",
        ));
    }
    Ok(())
}

fn validate_values(values: &[f64], expected: usize, path: &str) -> AexResult<()> {
    if values.len() != expected {
        return Err(AexError::validation(
            "INVALID_POLAR_TABLE_DIMENSIONS",
            path,
            format!("expected {expected} values"),
        ));
    }
    if values
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(AexError::validation(
            "INVALID_POLAR_TABLE_CELL",
            path,
            "table cells must be finite and positive",
        ));
    }
    Ok(())
}

fn validate_induced_schedule(table: &PolarTable, root: &str) -> AexResult<()> {
    match (&table.oswald_efficiency, &table.induced_drag_factor) {
        (Some(values), None) => {
            validate_values(
                values,
                table.mach.len(),
                &format!("{root}.oswald_efficiency"),
            )?;
            if values.iter().any(|value| *value > 1.0) {
                return Err(AexError::validation(
                    "INVALID_OSWALD_EFFICIENCY",
                    format!("{root}.oswald_efficiency"),
                    "Oswald efficiency must be in (0, 1]",
                ));
            }
            Ok(())
        }
        (None, Some(values)) => validate_values(
            values,
            table.mach.len(),
            &format!("{root}.induced_drag_factor"),
        ),
        _ => Err(AexError::validation(
            "POLAR_INDUCED_FIELD_EXCLUSIVITY",
            root,
            "exactly one of oswald_efficiency or induced_drag_factor is required",
        )),
    }
}
