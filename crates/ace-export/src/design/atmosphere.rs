/// Standard gravity, m/s².
const G0: f64 = 9.806_65;
/// Specific gas constant of air, J/(kg·K).
const R_AIR: f64 = 287.052_87;
/// Earth radius of the geopotential conversion, metres.
const EARTH_RADIUS_M: f64 = 6_356_766.0;
const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;
const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;
/// Temperature lapse of the troposphere, K per geopotential metre.
const LAPSE_K_M: f64 = 0.0065;
/// Geopotential height of the tropopause, metres.
const TROPOPAUSE_M: f64 = 11_000.0;
/// Top of the isothermal layer above it, metres.
const ISOTHERMAL_TOP_M: f64 = 20_000.0;
/// Bottom of the standard atmosphere's table, metres.
const LOWEST_M: f64 = -5_000.0;

/// ICAO standard density at geometric height `height_m`, for the two layers a
/// propeller aeroplane flies in; `None` outside them.
pub(super) fn isa_density_kg_m3(height_m: f64) -> Option<f64> {
    let geopotential = EARTH_RADIUS_M * height_m / (EARTH_RADIUS_M + height_m);
    if !(LOWEST_M..=ISOTHERMAL_TOP_M).contains(&geopotential) {
        return None;
    }
    let exponent = G0 / (LAPSE_K_M * R_AIR);
    let temperature_at = |height: f64| SEA_LEVEL_TEMPERATURE_K - LAPSE_K_M * height;
    let pressure_at = |height: f64| {
        SEA_LEVEL_PRESSURE_PA * (temperature_at(height) / SEA_LEVEL_TEMPERATURE_K).powf(exponent)
    };
    let (temperature, pressure) = if geopotential <= TROPOPAUSE_M {
        (temperature_at(geopotential), pressure_at(geopotential))
    } else {
        let temperature = temperature_at(TROPOPAUSE_M);
        let decay = -G0 * (geopotential - TROPOPAUSE_M) / (R_AIR * temperature);
        (temperature, pressure_at(TROPOPAUSE_M) * decay.exp())
    };
    Some(pressure / (R_AIR * temperature))
}
