use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::result::{AtmosphereState, ModelMetadata};

const STANDARD_GRAVITY: f64 = 9.806_65;
const SPECIFIC_GAS_CONSTANT: f64 = 287.052_87;
const HEAT_CAPACITY_RATIO: f64 = 1.4;
const EARTH_RADIUS_M: f64 = 6_356_766.0;
/// Lowest geometric altitude of the model.
pub(crate) const MINIMUM_ALTITUDE_M: f64 = -2_000.0;
/// Geometric top of the 1976 standard's hydrostatic layers (84 852 m geopotential).
pub(crate) const MAXIMUM_ALTITUDE_M: f64 = 86_000.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Isa1976 {
    temperature_offset_k: f64,
}

/// The atmosphere every native analysis of `scenario` uses: ISA shifted by the
/// mission's declared temperature deviation.
pub(crate) fn scenario_atmosphere(scenario: &crate::domain::schema::ResolvedScenario) -> Isa1976 {
    Isa1976::new(scenario.mission.isa_offset_k)
}

impl Isa1976 {
    pub(crate) fn new(temperature_offset_k: f64) -> Self {
        Self {
            temperature_offset_k,
        }
    }

    pub(crate) fn evaluate(&self, geometric_altitude_m: f64) -> AexResult<AtmosphereState> {
        if !(MINIMUM_ALTITUDE_M..=MAXIMUM_ALTITUDE_M).contains(&geometric_altitude_m) {
            return Err(AexError::analysis(
                "ATMOSPHERE_OUTSIDE_VALIDITY",
                format!(
                    "altitude {geometric_altitude_m} m is outside {MINIMUM_ALTITUDE_M} to {MAXIMUM_ALTITUDE_M} m"
                ),
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

/// US Standard Atmosphere 1976 layers: geopotential base height (m), tabulated base
/// temperature (K) and pressure (Pa), and lapse rate (K/m).
const LAYERS: [(f64, f64, f64, f64); 7] = [
    (0.0, 288.15, 101_325.0, -0.0065),
    (11_000.0, 216.65, 22_632.06, 0.0),
    (20_000.0, 216.65, 5_474.889, 0.001),
    (32_000.0, 228.65, 868.0187, 0.0028),
    (47_000.0, 270.65, 110.9063, 0.0),
    (51_000.0, 270.65, 66.93887, -0.0028),
    (71_000.0, 214.65, 3.956_420, -0.002),
];

/// Temperature and pressure at a geopotential altitude from the containing layer's
/// tabulated base state; altitudes below sea level extend the first layer.
fn pressure_temperature(altitude_m: f64) -> (f64, f64) {
    let (base, temperature, pressure, lapse) = LAYERS
        .iter()
        .rev()
        .find(|(base, ..)| altitude_m >= *base)
        .or_else(|| LAYERS.first())
        .copied()
        .unwrap_or((0.0, 288.15, 101_325.0, -0.0065));
    layer(temperature, pressure, lapse, altitude_m - base)
}

fn layer(base_temperature: f64, base_pressure: f64, lapse: f64, height_m: f64) -> (f64, f64) {
    if lapse == 0.0 {
        let exponent = -STANDARD_GRAVITY * height_m / (SPECIFIC_GAS_CONSTANT * base_temperature);
        return (base_temperature, base_pressure * exponent.exp());
    }
    let temperature = base_temperature + lapse * height_m;
    let exponent = -STANDARD_GRAVITY / (lapse * SPECIFIC_GAS_CONSTANT);
    (
        temperature,
        base_pressure * (temperature / base_temperature).powf(exponent),
    )
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
