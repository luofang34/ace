use ace_model::Unit;
use serde_json::Value;

use super::{Values, positive, require};
use crate::AceExportError;

const PISTON_ENGINE: &str = "piston_engine";
const AERODYNAMIC_POLARS: &str = "aerodynamic_polars";
/// Lift and drag full flap adds when the catalog has no polars: a plain flap
/// on a light aeroplane.
const DEFAULT_FLAPS: FlapDesign = FlapDesign {
    delta_cl: 0.6,
    delta_cd: 0.03,
};

/// The propeller a design installs, from its catalog entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropellerDesign {
    /// Diameter, metres.
    pub diameter_m: f64,
    /// Blades.
    pub blade_count: u32,
    /// Efficiency at the cruise point.
    pub cruise_efficiency: f64,
    /// Helical tip Mach number not to be exceeded.
    pub tip_mach_limit: f64,
}

/// A piston engine, from its catalog entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PistonDesign {
    /// Crankshaft speed of the rated power, rpm.
    pub rated_rpm: f64,
    /// `engine.power_lapse_exponent`: power falls as density ratio to this.
    pub power_lapse_exponent: f64,
}

/// What turns the propeller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Powerplant {
    /// A piston engine: the catalog has a `piston_engine` entry.
    Piston(PistonDesign),
    /// Anything else, flown as an electric motor of the same shaft power.
    Electric,
}

/// What full flap adds to the clean wing's coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlapDesign {
    /// Added lift coefficient.
    pub delta_cl: f64,
    /// Added drag coefficient.
    pub delta_cd: f64,
}

fn catalog_error(field: &'static str, detail: impl Into<String>) -> AceExportError {
    AceExportError::Catalog {
        field,
        detail: detail.into(),
    }
}

/// A catalog quantity such as `"1.93 m"`, read in the unit it is written in.
fn quantity(document: &Value, field: &'static str, suffix: &str) -> Result<f64, AceExportError> {
    let text = document
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| catalog_error(field, "missing"))?;
    let number = text
        .strip_suffix(suffix)
        .ok_or_else(|| catalog_error(field, format!("{text:?} is not in{suffix}")))?;
    number
        .parse()
        .map_err(|_| catalog_error(field, format!("{text:?} is not a number")))
}

fn number(document: &Value, field: &'static str) -> Result<f64, AceExportError> {
    document
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| catalog_error(field, "missing"))
}

/// The document of the catalog entry of `kind`.
fn entry<'a>(values: &Values<'a>, kind: &str) -> Option<&'a Value> {
    values
        .model
        .catalog
        .values()
        .find(|entry| entry.kind == kind)
        .map(|entry| &entry.document)
}

fn parameters(document: &Value) -> Result<&Value, AceExportError> {
    document
        .get("parameters")
        .ok_or_else(|| catalog_error("parameters", "missing"))
}

pub(super) fn propeller(values: &Values<'_>) -> Result<PropellerDesign, AceExportError> {
    let document = entry(values, "propeller")
        .ok_or_else(|| catalog_error("kind", "the catalog has no propeller"))?;
    let parameters = parameters(document)?;
    Ok(PropellerDesign {
        diameter_m: positive(
            "propeller diameter",
            quantity(parameters, "diameter", " m")?,
        )?,
        blade_count: number(parameters, "blade_count")? as u32,
        cruise_efficiency: {
            let efficiency = values.parameter("propeller.efficiency", Unit::Ratio)?;
            require(
                "propeller.efficiency",
                efficiency,
                efficiency > 0.0 && efficiency < 1.0,
                "between 0 and 1",
            )?
        },
        tip_mach_limit: positive("tip_mach_limit", number(parameters, "tip_mach_limit")?)?,
    })
}

pub(super) fn powerplant(values: &Values<'_>) -> Result<Powerplant, AceExportError> {
    let Some(document) = entry(values, PISTON_ENGINE) else {
        return Ok(Powerplant::Electric);
    };
    let rated_rpm = quantity(parameters(document)?, "rated_speed", " rpm")?;
    Ok(Powerplant::Piston(PistonDesign {
        rated_rpm: positive("rated_speed", rated_rpm)?,
        power_lapse_exponent: positive(
            "engine.power_lapse_exponent",
            values.parameter("engine.power_lapse_exponent", Unit::Ratio)?,
        )?,
    }))
}

/// What the landing polar adds to the clean wing, or the default without one.
pub(super) fn flaps(
    values: &Values<'_>,
    clean_cl_max: f64,
    clean_cd0: f64,
) -> Result<FlapDesign, AceExportError> {
    let Some(document) = entry(values, AERODYNAMIC_POLARS) else {
        return Ok(DEFAULT_FLAPS);
    };
    let landing = document
        .get("landing")
        .ok_or_else(|| catalog_error("landing", "missing"))?;
    let delta_cl = number(landing, "cl_max")? - clean_cl_max;
    let delta_cd = number(landing, "cd0")? - clean_cd0;
    Ok(FlapDesign {
        delta_cl: require("landing cl_max", delta_cl, delta_cl >= 0.0, "above clean")?,
        delta_cd: require("landing cd0", delta_cd, delta_cd >= 0.0, "above clean")?,
    })
}
