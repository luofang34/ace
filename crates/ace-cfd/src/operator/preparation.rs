use super::reconstruction::gradients;
use super::{FlowOperator, Workspace, axial, axisymmetry, pressure, reconstruction};
use crate::{CfdError, Conserved};

impl FlowOperator {
    pub(super) fn prepare(
        &self,
        state: &[Conserved],
        workspace: &mut Workspace,
    ) -> Result<(), CfdError> {
        self.check_count(state.len())?;
        workspace.mean_state.copy_from_slice(state);
        for (i, u) in state.iter().enumerate() {
            workspace.primitive[i] = self.gas.primitive(*u, i)?;
            workspace.conservative_primitive[i] = workspace.primitive[i];
            workspace.temperature_mean[i] = self.gas.temperature(workspace.primitive[i]);
            if u[3] != 0.0 {
                return Err(CfdError::Configuration(
                    "axisymmetric swirl is not supported".into(),
                ));
            }
        }
        gradients(
            self,
            &workspace.primitive,
            &workspace.temperature_mean,
            &mut workspace.gradient,
        );
        if self.mesh.cells[0].radial_moments.is_some() {
            axisymmetry::recover_mean_primitives(self, workspace);
            gradients(
                self,
                &workspace.primitive,
                &workspace.temperature_mean,
                &mut workspace.gradient,
            );
        }
        for (i, cell) in self.mesh.cells.iter().enumerate() {
            let pressure = workspace.primitive[i].pressure;
            workspace.shock_seed[i] = cell
                .faces
                .iter()
                .map(|id| {
                    let (other, _) = self.sample(&self.mesh.faces[*id], i, &workspace.primitive);
                    ((other.pressure - pressure).abs() / (other.pressure + pressure) - 0.05) / 0.15
                })
                .fold(0.0_f64, f64::max)
                .min(1.0);
        }
        workspace
            .shock_sensor
            .copy_from_slice(&workspace.shock_seed);
        // A transverse face also needs dissipation when its neighbor crosses a shock.
        for face in &self.mesh.faces {
            if let Some(n) = face.neighbor {
                workspace.shock_sensor[face.owner] =
                    workspace.shock_sensor[face.owner].max(workspace.shock_seed[n]);
                workspace.shock_sensor[n] =
                    workspace.shock_sensor[n].max(workspace.shock_seed[face.owner]);
            }
        }
        self.prepare_pressure(workspace);
        // Curved pressure polynomials consume the original limited
        // slopes; primitive face traces additionally bound the projected slopes.
        reconstruction::limit_shock_gradients(self, workspace);
        Ok(())
    }

    fn prepare_pressure(&self, workspace: &mut Workspace) {
        for i in 0..self.mesh.cells.len() {
            workspace.axial[i] = axial::fit(self, workspace, i);
            workspace.pressure[i] = pressure::coefficients(
                &self.pressure_stencils[i],
                |n| workspace.primitive[n].pressure,
                i,
            );
        }
        // Complete the indicators before limiting so each stencil sees the
        // same nonsmoothness evidence, regardless of cell traversal order.
        for i in 0..self.mesh.cells.len() {
            workspace.pressure_defect[i] = pressure::model_defect(
                &self.pressure_stencils[i],
                workspace,
                i,
                workspace.pressure[i],
            );
        }
        for i in 0..self.mesh.cells.len() {
            workspace.pressure[i] = pressure::fit(self, workspace, i);
        }
    }
}
