use super::powerplant::Powerplant;
use crate::domain::schema::AeroConfiguration;
use ace_engine::SolverError;
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

pub(super) struct FlightModel {
    pub area: f64,
    pub aspect_ratio: f64,
    pub aero: AeroConfiguration,
    pub powerplant: Powerplant,
    pub fuel_price: f64,
    pub maximum_step: f64,
}

impl FlightModel {
    pub fn new(inputs: &BTreeMap<String, Quantity>) -> Result<Self, SolverError> {
        use Unit::*;
        let area = positive(inputs, "area", SquareMeter)?;
        let span = positive(inputs, "span", Meter)?;
        let oswald = positive(inputs, "oswald", Ratio)?;
        if oswald > 1.0 {
            return Err(SolverError::Failed("Oswald efficiency exceeds one".into()));
        }
        let fuel_price = read(inputs, "fuel_price", DollarPerKilogram)?;
        if fuel_price < 0.0 {
            return Err(SolverError::Failed("negative fuel price".into()));
        }
        Ok(Self {
            area,
            aspect_ratio: span * span / area,
            aero: AeroConfiguration {
                cd0: positive(inputs, "cd0", Ratio)?,
                oswald_efficiency: oswald,
                cl_max: positive(inputs, "cl_max", Ratio)?,
                additional_cd: 0.0,
                mach_critical: None,
                wave_drag: None,
                polar_table: None,
            },
            powerplant: Powerplant::new(inputs)?,
            fuel_price,
            maximum_step: positive(inputs, "maximum_step", Second)?,
        })
    }
}

/// Optional along-track headwind stage input; negative is a tailwind.
pub(super) fn ground_speed(
    values: &BTreeMap<String, Quantity>,
    airspeed: f64,
) -> Result<f64, SolverError> {
    let headwind = values
        .get("headwind")
        .map(|value| value.to(Unit::MeterPerSecond).map(|value| value.value))
        .transpose()?
        .unwrap_or(0.0);
    let ground = airspeed - headwind;
    if ground <= 0.0 {
        return Err(SolverError::Failed(format!(
            "headwind {headwind} m/s leaves no ground speed at {airspeed} m/s TAS"
        )));
    }
    Ok(ground)
}

pub(super) fn read(
    values: &BTreeMap<String, Quantity>,
    key: &str,
    unit: Unit,
) -> Result<f64, SolverError> {
    Ok(values
        .get(key)
        .ok_or_else(|| SolverError::Failed(format!("missing flight input {key}")))?
        .to(unit)?
        .value)
}

pub(super) fn positive(
    values: &BTreeMap<String, Quantity>,
    key: &str,
    unit: Unit,
) -> Result<f64, SolverError> {
    let value = read(values, key, unit)?;
    if value <= 0.0 {
        return Err(SolverError::Failed(format!("{key} must be positive")));
    }
    Ok(value)
}
