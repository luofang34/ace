use super::{FlowOperator, Workspace};
use crate::Primitive;
use crate::mesh::{Boundary, Face};
use crate::vector::{Vector, add, dot, scale, sub};
use serde::{Deserialize, Serialize};

/// Outlet discretization for controlled boundary-sensitivity experiments.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outflow {
    /// Copy the interior state and impose zero normal viscous gradients.
    #[default]
    ZeroGradient,
    /// Extrapolate using gradients fitted to interior and wall samples only.
    OneSided,
    /// Decode conservative cell means for convection; retain one-sided viscous gradients.
    CellFlux,
    /// Decode conservative cell means for convection; impose zero normal viscous gradients.
    ZeroGradientCellFlux,
}

#[cfg(test)]
mod tests;

mod curved;

impl FlowOperator {
    pub(super) fn sample_temperature(
        face: &Face,
        cell: usize,
        temperatures: &[f64],
        boundary_temperature: f64,
    ) -> f64 {
        if let Some(other) = face.neighbor {
            return temperatures[if cell == face.owner {
                other
            } else {
                face.owner
            }];
        }
        match face.boundary {
            Boundary::Wall | Boundary::Inflow => boundary_temperature,
            Boundary::Axis | Boundary::Outflow => temperatures[cell],
        }
    }

    fn wall_normal_neighbor(&self, face: &Face) -> Option<usize> {
        let cell = &self.mesh.cells[face.owner];
        let inward = scale(face.normal, -1.0);
        cell.faces
            .iter()
            .filter_map(|id| {
                let f = &self.mesh.faces[*id];
                f.neighbor
                    .map(|n| if f.owner == face.owner { n } else { f.owner })
            })
            .find(|n| {
                let d = sub(self.mesh.cells[*n].center, cell.center);
                dot(d, inward) > 0.9 * crate::vector::norm(d)
            })
    }

    pub(super) fn extrapolated_outflow(&self, face: &Face) -> bool {
        face.neighbor.is_none()
            && matches!(face.boundary, Boundary::Outflow)
            && matches!(self.outflow, Outflow::OneSided | Outflow::CellFlux)
    }

    pub(super) fn cell_flux_outflow(&self, face: &Face) -> bool {
        face.neighbor.is_none()
            && matches!(face.boundary, Boundary::Outflow)
            && matches!(
                self.outflow,
                Outflow::CellFlux | Outflow::ZeroGradientCellFlux
            )
    }
    pub(super) fn wall_gradient(&self, face: &Face, workspace: &Workspace) -> [Vector; 4] {
        if let Some(moments) = self.mesh.cells[face.owner].radial_moments {
            return self.curved_wall_gradient(face, workspace, moments);
        }
        let cell = &self.mesh.cells[face.owner];
        let inward = scale(face.normal, -1.0);
        let d1 = dot(sub(cell.center, face.center), inward);
        let q1 = workspace.primitive[face.owner];
        let values = |q: Primitive| {
            [
                q.velocity[0],
                q.velocity[1],
                q.velocity[2],
                self.gas.temperature(q),
            ]
        };
        let a = values(q1);
        let wall = [0.0, 0.0, 0.0, self.wall_temperature];
        let next = self.wall_normal_neighbor(face);
        let derivative = if let Some(n) = next {
            let d2 = dot(sub(self.mesh.cells[n].center, face.center), inward);
            let b = values(workspace.primitive[n]);
            std::array::from_fn(|k| {
                (d2 * d2 * (a[k] - wall[k]) - d1 * d1 * (b[k] - wall[k])) / (d1 * d2 * (d2 - d1))
            })
        } else {
            std::array::from_fn(|k| (a[k] - wall[k]) / d1)
        };
        // The no-slip isothermal wall has zero tangential derivatives.
        derivative.map(|d| scale(inward, d))
    }

    pub(crate) fn sample(
        &self,
        face: &Face,
        cell: usize,
        states: &[Primitive],
    ) -> (Primitive, Vector) {
        if let Some(other) = face.neighbor {
            let n = if cell == face.owner {
                other
            } else {
                face.owner
            };
            return (states[n], self.mesh.cells[n].center);
        }
        let q = states[cell];
        let center = self.mesh.cells[cell].center;
        let mirror = add(
            center,
            scale(
                face.normal,
                2.0 * dot(sub(face.center, center), face.normal),
            ),
        );
        match face.boundary {
            Boundary::Wall => {
                let pressure = self
                    .curved_wall_pressure(face, states)
                    .unwrap_or(q.pressure);
                (
                    Primitive {
                        density: pressure / (self.gas.gas_constant * self.wall_temperature),
                        velocity: [0.0; 3],
                        pressure,
                    },
                    self.mesh.cells[cell]
                        .radial_moments
                        .map_or(face.center, |m| {
                            // The boundary sample shares the cell's angular averaging;
                            // its separation is the mean distance from the true sphere.
                            sub(
                                center,
                                scale(center, m.scalar[0] / crate::vector::norm(center)),
                            )
                        }),
                )
            }
            Boundary::Axis => (
                Primitive {
                    velocity: [q.velocity[0], -q.velocity[1], 0.0],
                    ..q
                },
                mirror,
            ),
            Boundary::Inflow => (self.inflow, face.center),
            Boundary::Outflow => (q, mirror),
        }
    }
}
