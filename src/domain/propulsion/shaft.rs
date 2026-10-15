use serde::Serialize;

use super::PropulsionMode;

/// Shaft-power variation with air density ratio sigma = rho / rho0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "model", rename_all = "snake_case")]
pub(crate) enum PowerLapse {
    /// Naturally aspirated: P / P_rated = sigma^exponent at every altitude.
    DensityPower {
        exponent: f64,
        minimum_power_fraction: Option<f64>,
    },
    /// Turbocharged or flat-rated: sigma^exponent_below up to the critical altitude,
    /// then continuing as (sigma / sigma_critical)^exponent_above.
    CriticalAltitude {
        critical_altitude_m: f64,
        critical_density_ratio: f64,
        exponent_below: f64,
        exponent_above: f64,
        minimum_power_fraction: Option<f64>,
    },
}

/// Available shaft-power fraction and whether a declared floor supplied it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LapseFraction {
    pub(crate) fraction: f64,
    pub(crate) floored: bool,
}

impl PowerLapse {
    pub(crate) fn fraction(&self, density_ratio: f64) -> LapseFraction {
        let sigma = density_ratio.max(0.0);
        let (lapse, floor) = match *self {
            Self::DensityPower {
                exponent,
                minimum_power_fraction,
            } => (sigma.powf(exponent), minimum_power_fraction),
            Self::CriticalAltitude {
                critical_density_ratio,
                exponent_below,
                exponent_above,
                minimum_power_fraction,
                ..
            } => {
                let lapse = if sigma >= critical_density_ratio {
                    sigma.powf(exponent_below)
                } else {
                    critical_density_ratio.powf(exponent_below)
                        * (sigma / critical_density_ratio).powf(exponent_above)
                };
                (lapse, minimum_power_fraction)
            }
        };
        match floor {
            Some(floor) if floor > lapse => LapseFraction {
                fraction: floor,
                floored: true,
            },
            _ => LapseFraction {
                fraction: lapse,
                floored: false,
            },
        }
    }
}

/// Mode BSFC with an optional declared linear altitude correction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct ShaftBsfc {
    pub(crate) takeoff_kg_kwh: f64,
    pub(crate) cruise_kg_kwh: f64,
    pub(crate) economy_kg_kwh: f64,
    /// Fractional BSFC change per 1000 m; absent means BSFC is altitude independent.
    pub(crate) altitude_slope_per_km: Option<f64>,
}

impl ShaftBsfc {
    pub(crate) fn at(&self, mode: PropulsionMode, altitude_m: f64) -> f64 {
        let base = match mode {
            PropulsionMode::Takeoff | PropulsionMode::Climb => self.takeoff_kg_kwh,
            PropulsionMode::Cruise => self.cruise_kg_kwh,
            PropulsionMode::Economy => self.economy_kg_kwh,
        };
        base * (1.0 + self.altitude_slope_per_km.unwrap_or(0.0) * altitude_m / 1000.0)
    }
}

#[cfg(test)]
mod tests;
