use super::{Boundary, FlowOperator};
use crate::{CfdError, Conserved};
use serde::Serialize;

/// Discrete energy budget over the full revolved domain, in watts.
#[derive(Debug, Serialize)]
pub struct EnergyBalance {
    /// Outward energy flux through the solid wall, positive when heating it.
    pub wall_power: f64,
    /// Outward total energy flux through the far-field boundary.
    pub inflow_power: f64,
    /// Outward total energy flux through the equatorial boundary.
    pub outflow_power: f64,
    /// Outward total energy flux through the symmetry axis (zero area).
    pub axis_power: f64,
    /// Volume integral of the energy time derivative, including its sign.
    pub volume_energy_rate: f64,
    /// Volume integral of the absolute energy time derivative.
    pub absolute_volume_energy_rate: f64,
    /// Sum of outward power and volume energy rate; roundoff for a conservative operator.
    pub closure_error: f64,
}

impl FlowOperator {
    /// Audit the same second-order face fluxes used by the residual, without advancing time.
    pub fn energy_balance(&self, state: &[Conserved]) -> Result<EnergyBalance, CfdError> {
        let mut workspace = self.workspace();
        self.evaluate(state, true, &mut workspace)?;
        let mut power = [0.0; 4];
        for face in self
            .mesh
            .faces
            .iter()
            .filter(|face| face.neighbor.is_none())
        {
            let index = match face.boundary {
                Boundary::Wall => 0,
                Boundary::Inflow => 1,
                Boundary::Outflow => 2,
                Boundary::Axis => 3,
            };
            power[index] +=
                self.integrated_face_flux(face, true, &workspace)[4] * std::f64::consts::TAU;
        }
        let rates = self
            .mesh
            .cells
            .iter()
            .zip(&workspace.residual)
            .map(|(c, r)| r[4] * c.volume * std::f64::consts::TAU);
        let volume_energy_rate: f64 = rates.clone().sum();
        Ok(EnergyBalance {
            wall_power: power[0],
            inflow_power: power[1],
            outflow_power: power[2],
            axis_power: power[3],
            volume_energy_rate,
            absolute_volume_energy_rate: rates.map(f64::abs).sum(),
            closure_error: power.iter().sum::<f64>() + volume_energy_rate,
        })
    }
}

#[cfg(test)]
mod tests;
