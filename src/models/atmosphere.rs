use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::result::{AtmosphereState, ModelMetadata};

const STANDARD_GRAVITY: f64 = 9.806_65;
const SPECIFIC_GAS_CONSTANT: f64 = 287.052_87;
const HEAT_CAPACITY_RATIO: f64 = 1.4;
const EARTH_RADIUS_M: f64 = 6_356_766.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Isa1976 {
    temperature_offset_k: f64,
}

impl Isa1976 {
    pub(crate) fn new(temperature_offset_k: f64) -> Self {
        Self {
            temperature_offset_k,
        }
    }

    pub(crate) fn evaluate(&self, geometric_altitude_m: f64) -> AexResult<AtmosphereState> {
        if !(-2000.0..=20_000.0).contains(&geometric_altitude_m) {
            return Err(AexError::analysis(
                "ATMOSPHERE_OUTSIDE_VALIDITY",
                format!("altitude {geometric_altitude_m} m is outside -2000 to 20000 m"),
            ));
        }
        let geopotential_altitude =
            EARTH_RADIUS_M * geometric_altitude_m / (EARTH_RADIUS_M + geometric_altitude_m);
        let (base_temperature, pressure) = pressure_temperature(geopotential_altitude);
        let temperature = base_temperature + self.temperature_offset_k;
        if temperature <= 0.0 {
            return Err(AexError::analysis(
                "INVALID_TEMPERATURE_OFFSET",
                "ISA temperature offset produces a non-positive temperature",
            ));
        }
        let density = pressure / (SPECIFIC_GAS_CONSTANT * temperature);
        let speed_of_sound = (HEAT_CAPACITY_RATIO * SPECIFIC_GAS_CONSTANT * temperature).sqrt();
        let dynamic_viscosity = sutherland_viscosity(temperature);
        Ok(AtmosphereState {
            altitude_m: geometric_altitude_m,
            temperature_k: temperature,
            pressure_pa: pressure,
            density_kg_m3: density,
            speed_of_sound_m_s: speed_of_sound,
            dynamic_viscosity_pa_s: dynamic_viscosity,
            kinematic_viscosity_m2_s: dynamic_viscosity / density,
            model: ModelMetadata {
                model_id: "atmosphere.isa1976".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 1,
                validity_status: "valid".to_owned(),
            },
            warnings: Vec::<Diagnostic>::new(),
        })
    }
}

fn pressure_temperature(altitude_m: f64) -> (f64, f64) {
    let sea_level_temperature = 288.15;
    let sea_level_pressure = 101_325.0;
    let lapse = -0.0065;
    if altitude_m <= 11_000.0 {
        let temperature = sea_level_temperature + lapse * altitude_m;
        let exponent = -STANDARD_GRAVITY / (lapse * SPECIFIC_GAS_CONSTANT);
        let pressure = sea_level_pressure * (temperature / sea_level_temperature).powf(exponent);
        (temperature, pressure)
    } else {
        let tropopause_temperature = 216.65;
        let tropopause_pressure = 22_632.06;
        let exponent = -STANDARD_GRAVITY * (altitude_m - 11_000.0)
            / (SPECIFIC_GAS_CONSTANT * tropopause_temperature);
        (tropopause_temperature, tropopause_pressure * exponent.exp())
    }
}

fn sutherland_viscosity(temperature_k: f64) -> f64 {
    let reference_viscosity = 1.716e-5;
    let reference_temperature = 273.15;
    let sutherland_temperature = 110.4;
    reference_viscosity
        * (temperature_k / reference_temperature).powf(1.5)
        * (reference_temperature + sutherland_temperature)
        / (temperature_k + sutherland_temperature)
}

#[cfg(test)]
mod tests;
