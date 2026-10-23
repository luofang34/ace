use crate::CfdError;
use crate::vector::{Vector, dot};
use serde::{Deserialize, Serialize};

/// Conservative density, three momentum densities, and total energy density (SI).
pub type Conserved = [f64; 5];

/// Calorically perfect gas with constant molecular transport coefficients.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdealGas {
    /// Ratio of specific heats.
    pub gamma: f64,
    /// Specific gas constant in J/(kg K).
    pub gas_constant: f64,
    /// Dynamic viscosity in Pa s.
    pub viscosity: f64,
    /// Thermal conductivity in W/(m K).
    pub conductivity: f64,
}

/// Cell primitive variables in SI units, independent of mesh topology.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Primitive {
    /// Density in kg/m³.
    pub density: f64,
    /// Cartesian velocity in m/s; axisymmetric runs require zero azimuthal speed.
    pub velocity: Vector,
    /// Static pressure in Pa.
    pub pressure: f64,
}

impl IdealGas {
    /// Check positivity and finite material coefficients.
    pub fn validate(self) -> Result<(), CfdError> {
        let values = [
            self.gamma,
            self.gas_constant,
            self.viscosity,
            self.conductivity,
        ];
        if values.iter().any(|v| !v.is_finite() || *v <= 0.0) || self.gamma <= 1.0 {
            return Err(CfdError::Configuration(
                "positive finite gas properties and gamma > 1 required".into(),
            ));
        }
        Ok(())
    }

    /// Static temperature in K.
    pub fn temperature(self, state: Primitive) -> f64 {
        state.pressure / (state.density * self.gas_constant)
    }

    /// Specific heat at constant pressure in J/(kg K).
    pub fn cp(self) -> f64 {
        self.gamma * self.gas_constant / (self.gamma - 1.0)
    }

    /// Acoustic wave speed in m/s.
    pub fn sound_speed(self, state: Primitive) -> f64 {
        (self.gamma * state.pressure / state.density).sqrt()
    }

    /// Convert primitive variables to conservative densities.
    pub fn conservative(self, state: Primitive) -> Conserved {
        let [u, v, w] = state.velocity;
        let rho = state.density;
        [
            rho,
            rho * u,
            rho * v,
            rho * w,
            state.pressure / (self.gamma - 1.0) + 0.5 * rho * dot(state.velocity, state.velocity),
        ]
    }

    /// Recover primitive variables, rejecting nonphysical states.
    pub fn primitive(self, state: Conserved, cell: usize) -> Result<Primitive, CfdError> {
        let density = state[0];
        let velocity = [state[1] / density, state[2] / density, state[3] / density];
        let pressure = (self.gamma - 1.0) * (state[4] - 0.5 * density * dot(velocity, velocity));
        if state.iter().any(|v| !v.is_finite()) || density <= 0.0 || pressure <= 0.0 {
            return Err(CfdError::State {
                cell,
                density,
                pressure,
            });
        }
        Ok(Primitive {
            density,
            velocity,
            pressure,
        })
    }
}

impl Primitive {
    pub(crate) fn components(self) -> [f64; 5] {
        [
            self.density,
            self.velocity[0],
            self.velocity[1],
            self.velocity[2],
            self.pressure,
        ]
    }

    pub(crate) fn from_components(q: [f64; 5]) -> Self {
        Self {
            density: q[0],
            velocity: [q[1], q[2], q[3]],
            pressure: q[4],
        }
    }
}

#[cfg(test)]
mod tests;
