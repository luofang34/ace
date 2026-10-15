use crate::domain::schema::{AeroConfiguration, SimpleTurbofanDeck, TurbofanProfile};
use ace_engine::SolverError;
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

pub(super) struct FlightModel {
    pub area: f64,
    pub aspect_ratio: f64,
    pub aero: AeroConfiguration,
    pub engine: TurbofanProfile,
    pub available_thrust: f64,
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
        let available_thrust = positive(inputs, "available_thrust", Newton)?;
        let tsfc = positive(inputs, "tsfc", KilogramPerNewtonSecond)?;
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
            engine: engine(tsfc, available_thrust),
            available_thrust,
            fuel_price,
            maximum_step: positive(inputs, "maximum_step", Second)?,
        })
    }
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

fn engine(tsfc: f64, thrust: f64) -> TurbofanProfile {
    TurbofanProfile {
        id: "canonical_phase_engine".into(),
        version: 1,
        model: "propulsion.simple_turbofan".into(),
        source: "canonical phase input ports".into(),
        confidence: "conceptual".into(),
        dry_mass_kg: 0.0,
        bypass_ratio: None,
        thrust_loss_fraction: 0.0,
        nacelle_drag_area_m2: 0.0,
        overall_length_m: None,
        maximum_diameter_m: None,
        table_deck: None,
        reference_thrust_n: thrust,
        simple_deck: Some(SimpleTurbofanDeck {
            sea_level_static_thrust_n: thrust,
            altitude_exponent: 0.0,
            mach_linear_coefficient: 0.0,
            minimum_thrust_fraction: 1.0,
            tsfc_takeoff_kg_n_hr: tsfc * 3600.0,
            tsfc_cruise_kg_n_hr: tsfc * 3600.0,
            cruise_reference_altitude_m: 0.0,
            cruise_reference_mach: 0.0,
            maximum_mach: 0.9,
            maximum_altitude_m: 20_000.0,
        }),
    }
}
