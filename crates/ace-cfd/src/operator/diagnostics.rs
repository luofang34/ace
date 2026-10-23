use super::FlowOperator;
use crate::mesh::Boundary;
use crate::vector::Vector;
use crate::{CfdError, Conserved, Primitive};
use serde::Serialize;

/// Read-only state and spatial-operator evidence for one finite-volume cell.
#[derive(Debug, Serialize)]
pub struct CellDiagnostic {
    /// Cell index in mesh order.
    pub cell: usize,
    /// Volume centroid in m.
    pub center: Vector,
    /// Original conservative cell mean, before primitive reconstruction.
    pub mean_state: Conserved,
    /// Primitive quantities used by the spatial operator.
    pub primitive: Primitive,
    /// Volume-mean temperature used by viscous reconstruction, in K.
    pub temperature_mean: f64,
    /// Physical gradients of density, three velocities, pressure and temperature.
    pub gradient: [Vector; 6],
    /// Gradients before limiting in the even scalar and odd radial-velocity coordinates.
    /// The radial velocity entry differentiates v/y; other entries differentiate
    /// the primitive quantities with respect to x and y²/(2*cell-center y).
    pub regular_gradient: [Vector; 6],
    /// Convective gradients after limiting and shock projection, in regular
    /// coordinates for curved cells and Cartesian coordinates for planar cells.
    /// The entries are density, three velocities and pressure; curved pressure
    /// traces instead use `pressure_coefficients`.
    pub limited_gradient: [Vector; 5],
    /// Curved-cell pressure polynomial used at faces and in volume sources.
    /// Its basis is x, z, x²/2, xz, z²/2, x³/6, x²z/2, x⁴/24, with each monomial
    /// centered by its volume mean. Here x is measured from the volume centroid
    /// and z=(y²-mean(y²))/(2*cell-center y).
    /// Planar cells use only the first two coefficients, for Cartesian x and y.
    pub pressure_coefficients: [f64; 8],
    /// Squared relative pressure-fit defect on held-out cell means, bounded to `[0,1]`.
    pub pressure_model_defect: f64,
    /// Continuous pressure sensor used for shock dissipation.
    pub shock_sensor: f64,
    /// Unscaled conservative time derivative, including axisymmetric sources.
    pub residual: Conserved,
    /// Local explicit step estimate before multiplication by the Courant factor.
    pub time_step: f64,
}

/// States and flux at an actual face integration point, oriented out of its owner.
#[derive(Debug, Serialize)]
pub struct FaceDiagnostic {
    /// Mesh face index, shared by its quadrature points.
    pub face: usize,
    /// Owner cell index.
    pub owner: usize,
    /// Interior neighbour, absent on the domain boundary.
    pub neighbor: Option<usize>,
    /// Integration point in m.
    pub center: Vector,
    /// Outward unit normal of the owner.
    pub normal: Vector,
    /// Quadrature weight including the cylindrical radius, per radian in m².
    pub area: f64,
    /// Reconstructed owner state; curved wall pressure fluxes use its pressure.
    pub left: Primitive,
    /// Reconstructed neighbour or exterior state; absent for an imposed wall flux.
    pub right: Option<Primitive>,
    /// Sensor supplied to the convective flux.
    pub shock_sensor: f64,
    /// Total flux per unit area, including viscosity, used by the residual.
    pub total_flux: Conserved,
}

impl FlowOperator {
    /// Expose the actual face quadrature and reconstruction without advancing a state.
    pub fn face_diagnostics(&self, state: &[Conserved]) -> Result<Vec<FaceDiagnostic>, CfdError> {
        let mut workspace = self.workspace();
        self.prepare(state, &mut workspace)?;
        let mut output = Vec::new();
        for (id, face) in self.mesh.faces.iter().enumerate() {
            for point in face.integration_points() {
                let face = face.at_point(point);
                let left = self.face_state(&face, face.owner, true, &workspace);
                let right = if let Some(n) = face.neighbor {
                    Some(self.face_state(&face, n, true, &workspace))
                } else {
                    match face.boundary {
                        Boundary::Wall | Boundary::Axis => None,
                        Boundary::Inflow => Some(self.inflow),
                        Boundary::Outflow => Some(left),
                    }
                };
                output.push(FaceDiagnostic {
                    face: id,
                    owner: face.owner,
                    neighbor: face.neighbor,
                    center: face.center,
                    normal: face.normal,
                    area: face.area,
                    left,
                    right,
                    shock_sensor: workspace.shock_sensor[face.owner]
                        .max(face.neighbor.map_or(0.0, |n| workspace.shock_sensor[n])),
                    total_flux: self.face_flux(&face, true, &workspace),
                });
            }
        }
        Ok(output)
    }

    /// Evaluate and expose the spatial residual without advancing or modifying a state.
    pub fn cell_diagnostics(&self, state: &[Conserved]) -> Result<Vec<CellDiagnostic>, CfdError> {
        let mut workspace = self.workspace();
        self.evaluate(state, true, &mut workspace)?;
        Ok(self
            .mesh
            .cells
            .iter()
            .enumerate()
            .map(|(i, cell)| CellDiagnostic {
                cell: i,
                center: cell.center,
                mean_state: state[i],
                primitive: workspace.primitive[i],
                temperature_mean: workspace.temperature_mean[i],
                gradient: workspace.gradient[i].raw,
                regular_gradient: workspace.gradient[i].regular,
                limited_gradient: workspace.gradient[i].limited,
                pressure_coefficients: workspace.pressure[i],
                pressure_model_defect: workspace.pressure_defect[i],
                shock_sensor: workspace.shock_sensor[i],
                residual: workspace.residual[i],
                time_step: workspace.time_step[i],
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
