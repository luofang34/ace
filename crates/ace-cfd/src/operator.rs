use crate::flux::{Convection, ausm, ausmdv, hllc, hlle, viscous};
use crate::mesh::{Boundary, Face};
use crate::vector::{Vector, add, dot, scale, sub};
use crate::{CfdError, Conserved, IdealGas, Mesh, Primitive};
use serde::Serialize;

mod axial;
mod axisymmetry;
mod balance;
mod preparation;
mod pressure;
mod quadratic;
pub use balance::EnergyBalance;
mod diagnostics;
pub use diagnostics::{CellDiagnostic, FaceDiagnostic};
mod boundary;
pub use boundary::Outflow;
mod reconstruction;
use reconstruction::{Gradient, reconstruct, reconstruct_curved, thermal_gradient};

/// Heat entering a stationary isothermal wall; positive values heat the body.
#[derive(Debug, Clone, Serialize)]
pub struct WallSample {
    /// Angular position from the stagnation axis in radians.
    pub theta: f64,
    /// Area-weighted theta squared at the heat-flux quadrature points, in rad².
    pub theta_squared_mean: f64,
    /// Area centroid of the wall face in m.
    pub position: Vector,
    /// Area-averaged heat flux into the body in W/m².
    pub heat_flux: f64,
    /// Wall pressure in Pa.
    pub pressure: f64,
    /// Revolved face area in m².
    pub area: f64,
}

/// Spatial Navier–Stokes residual, independent of the steady iteration algorithm.
pub struct FlowOperator {
    pub(crate) mesh: Mesh,
    pub(crate) gas: IdealGas,
    pub(crate) inflow: Primitive,
    pub(crate) wall_temperature: f64,
    convection: Convection,
    outflow: Outflow,
    quadratic_stencils: Vec<quadratic::Stencil>,
    pressure_stencils: Vec<pressure::Stencil>,
}

pub(crate) struct Workspace {
    mean_state: Vec<Conserved>,
    primitive: Vec<Primitive>,
    temperature_mean: Vec<f64>,
    gradient: Vec<Gradient>,
    axial: Vec<[f64; 8]>,
    pressure: Vec<[f64; 8]>,
    pressure_defect: Vec<f64>,
    shock_sensor: Vec<f64>,
    shock_seed: Vec<f64>,
    pub residual: Vec<Conserved>,
    pub time_step: Vec<f64>,
}

impl FlowOperator {
    /// Set material and boundary data for a no-swirl axisymmetric flow.
    pub fn new(
        mesh: Mesh,
        gas: IdealGas,
        inflow: Primitive,
        wall_temperature: f64,
    ) -> Result<Self, CfdError> {
        gas.validate()?;
        gas.primitive(gas.conservative(inflow), 0)?;
        if !wall_temperature.is_finite() || wall_temperature <= 0.0 || inflow.velocity[2] != 0.0 {
            return Err(CfdError::Configuration(
                "positive wall temperature and zero swirl required".into(),
            ));
        }
        let quadratic_stencils = quadratic::stencils(&mesh)?;
        let pressure_stencils = pressure::stencils(&mesh)?;
        Ok(Self {
            quadratic_stencils,
            pressure_stencils,
            mesh,
            gas,
            inflow,
            wall_temperature,
            convection: Convection::default(),
            outflow: Outflow::default(),
        })
    }

    /// Choose the numerical convective flux for a verification experiment.
    pub fn with_convection(mut self, convection: Convection) -> Self {
        self.convection = convection;
        self
    }

    /// Select the outlet stencil without changing the imposed inflow or wall data.
    pub fn with_outflow(mut self, outflow: Outflow) -> Self {
        self.outflow = outflow;
        self
    }

    /// Uniform free-stream initial state.
    pub fn initial_state(&self) -> Vec<Conserved> {
        vec![self.gas.conservative(self.inflow); self.mesh.cell_count()]
    }

    /// Evaluate dU/dt without altering the input or applying time-step scaling.
    pub fn residual(
        &self,
        state: &[Conserved],
        second_order: bool,
    ) -> Result<Vec<Conserved>, CfdError> {
        let mut workspace = self.workspace();
        self.evaluate(state, second_order, &mut workspace)?;
        Ok(workspace.residual)
    }

    /// Centered directional residual difference for sensitivity verification.
    /// `epsilon` scales the supplied direction; shocks and limiter switches are nonsmooth.
    pub fn jacobian_vector(
        &self,
        state: &[Conserved],
        direction: &[Conserved],
        epsilon: f64,
    ) -> Result<Vec<Conserved>, CfdError> {
        self.check_count(state.len())?;
        self.check_count(direction.len())?;
        if !epsilon.is_finite() || epsilon <= 0.0 {
            return Err(CfdError::Configuration(
                "positive finite difference increment required".into(),
            ));
        }
        let perturb = |sign: f64| {
            state
                .iter()
                .zip(direction)
                .map(|(u, d)| std::array::from_fn(|k| u[k] + sign * epsilon * d[k]))
                .collect::<Vec<_>>()
        };
        let plus = self.residual(&perturb(1.0), true)?;
        let minus = self.residual(&perturb(-1.0), true)?;
        Ok(plus
            .iter()
            .zip(minus)
            .map(|(p, m)| std::array::from_fn(|k| (p[k] - m[k]) / (2.0 * epsilon)))
            .collect())
    }

    /// Recover wall loads using exactly the conductive face flux used by the residual.
    pub fn wall_samples(&self, state: &[Conserved]) -> Result<Vec<WallSample>, CfdError> {
        let mut workspace = self.workspace();
        self.prepare(state, &mut workspace)?;
        Ok(self
            .mesh
            .faces
            .iter()
            .filter(|f| f.neighbor.is_none() && matches!(f.boundary, Boundary::Wall))
            .map(|f| WallSample {
                theta: f.center[1].atan2(-f.center[0]),
                theta_squared_mean: f
                    .integration_points()
                    .map(|p| p.center[1].atan2(-p.center[0]).powi(2) * p.area)
                    .sum::<f64>()
                    / f.area,
                position: f.center,
                heat_flux: self.integrated_face_flux(f, true, &workspace)[4] / f.area,
                pressure: if self.mesh.cells[f.owner].radial_moments.is_some() {
                    f.integration_points()
                        .map(|p| {
                            self.face_state(&f.at_point(p), f.owner, true, &workspace)
                                .pressure
                                * p.area
                        })
                        .sum::<f64>()
                        / f.area
                } else {
                    workspace.primitive[f.owner].pressure
                },
                area: f.area * std::f64::consts::TAU,
            })
            .collect())
    }

    pub(crate) fn workspace(&self) -> Workspace {
        let n = self.mesh.cell_count();
        Workspace {
            mean_state: vec![[0.0; 5]; n],
            primitive: vec![self.inflow; n],
            temperature_mean: vec![self.gas.temperature(self.inflow); n],
            gradient: vec![Gradient::default(); n],
            axial: vec![[0.0; 8]; n],
            pressure: vec![[0.0; 8]; n],
            pressure_defect: vec![0.0; n],
            shock_sensor: vec![0.0; n],
            shock_seed: vec![0.0; n],
            residual: vec![[0.0; 5]; n],
            time_step: vec![0.0; n],
        }
    }

    fn check_count(&self, count: usize) -> Result<(), CfdError> {
        if count != self.mesh.cell_count() {
            return Err(CfdError::StateCount {
                expected: self.mesh.cell_count(),
                actual: count,
            });
        }
        Ok(())
    }

    pub(crate) fn evaluate(
        &self,
        state: &[Conserved],
        second_order: bool,
        workspace: &mut Workspace,
    ) -> Result<(), CfdError> {
        self.prepare(state, workspace)?;
        workspace.residual.fill([0.0; 5]);
        workspace.time_step.fill(0.0);
        for face in &self.mesh.faces {
            let flux = self.integrated_face_flux(face, second_order, workspace);
            for (k, value) in flux.into_iter().enumerate() {
                workspace.residual[face.owner][k] -= value;
                if let Some(n) = face.neighbor {
                    workspace.residual[n][k] += value;
                }
            }
            self.spectral_radius(face, workspace);
        }
        for (i, cell) in self.mesh.cells.iter().enumerate() {
            let q = workspace.primitive[i];
            let tau = axial::hoop_stress(self, workspace, i);
            let source_pressure = if second_order {
                pressure::planar_mean(cell, q.pressure, workspace.pressure[i])
            } else {
                q.pressure
            };
            workspace.residual[i][2] += (source_pressure - tau) * cell.planar_area;
            for value in &mut workspace.residual[i] {
                *value /= cell.volume;
            }
            workspace.time_step[i] = cell.volume / workspace.time_step[i];
        }
        Ok(())
    }

    fn integrated_face_flux(
        &self,
        face: &Face,
        second_order: bool,
        workspace: &Workspace,
    ) -> Conserved {
        let mut integrated = [0.0; 5];
        for point in face.integration_points() {
            let flux = self.face_flux(&face.at_point(point), second_order, workspace);
            for (total, value) in integrated.iter_mut().zip(flux) {
                *total += value * point.area;
            }
        }
        integrated
    }

    fn face_flux(&self, face: &Face, second_order: bool, workspace: &Workspace) -> Conserved {
        let q = workspace.primitive[face.owner];
        let g = workspace.gradient[face.owner];
        let left = self.face_state(face, face.owner, second_order, workspace);
        let wall =
            face.neighbor.is_none() && matches!(face.boundary, Boundary::Wall | Boundary::Axis);
        let mut flux = if wall {
            let pressure = if self.mesh.cells[face.owner].radial_moments.is_some() {
                left.pressure
            } else {
                q.pressure
            };
            [
                0.0,
                pressure * face.normal[0],
                pressure * face.normal[1],
                0.0,
                0.0,
            ]
        } else {
            let right = if let Some(n) = face.neighbor {
                self.face_state(face, n, second_order, workspace)
            } else if matches!(face.boundary, Boundary::Inflow) {
                self.inflow
            } else {
                left
            };
            let sensor = workspace.shock_sensor[face.owner]
                .max(face.neighbor.map_or(0.0, |n| workspace.shock_sensor[n]));
            self.convection(left, right, face.normal, sensor)
        };
        let velocity = self.viscous_velocity(face, workspace);
        let radial_strain = if face.center[1] > 0.0 {
            velocity[1] / face.center[1]
        } else {
            g.raw[2][1]
        };
        let vf = viscous(
            self.gas,
            velocity,
            self.face_gradient(face, q, g, workspace),
            face.normal,
            radial_strain,
        );
        for k in 0..5 {
            flux[k] -= vf[k];
        }
        flux
    }

    fn face_state(
        &self,
        face: &Face,
        cell: usize,
        second_order: bool,
        workspace: &Workspace,
    ) -> Primitive {
        let q = workspace.primitive[cell];
        if !second_order || self.cell_flux_outflow(face) {
            return q;
        }
        let g = reconstruction::flatten(workspace.gradient[cell], workspace.shock_sensor[cell]);
        let delta = sub(face.center, self.mesh.cells[cell].center);
        let geometry = &self.mesh.cells[cell];
        if geometry.radial_moments.is_some() {
            let mut state = reconstruct_curved(q, g, geometry, face.center);
            state.pressure =
                q.pressure + pressure::change(geometry, workspace.pressure[cell], face.center);
            state
        } else {
            reconstruct(q, g, delta)
        }
    }

    fn convection(
        &self,
        left: Primitive,
        right: Primitive,
        normal: Vector,
        sensor: f64,
    ) -> Conserved {
        match self.convection {
            Convection::HybridHllc => {
                let contact = hllc(self.gas, left, right, normal);
                let shock = hlle(self.gas, left, right, normal);
                std::array::from_fn(|k| sensor * shock[k] + (1.0 - sensor) * contact[k])
            }
            Convection::AusmPlusUp => ausm(
                self.gas,
                left,
                right,
                normal,
                crate::vector::norm(self.inflow.velocity) / self.gas.sound_speed(self.inflow),
            ),
            Convection::Ausmdv => ausmdv(self.gas, left, right, normal, sensor),
            Convection::PureAusmdv => ausmdv(self.gas, left, right, normal, 0.0),
        }
    }

    fn viscous_velocity(&self, face: &Face, workspace: &Workspace) -> Vector {
        if face.neighbor.is_none() && matches!(face.boundary, Boundary::Wall | Boundary::Axis) {
            return [0.0; 3];
        }
        // Volume centroids are not symmetric about a face, especially beside the axis.
        let at_face = |i: usize| {
            axisymmetry::velocity(
                workspace.primitive[i],
                &self.mesh.cells[i],
                face.center,
                workspace.gradient[i].regular,
            )
        };
        let left = at_face(face.owner);
        if let Some(n) = face.neighbor {
            scale(add(left, at_face(n)), 0.5)
        } else if matches!(face.boundary, Boundary::Inflow) {
            self.inflow.velocity
        } else {
            left
        }
    }

    fn face_gradient(
        &self,
        face: &Face,
        q: Primitive,
        g: Gradient,
        workspace: &Workspace,
    ) -> [Vector; 4] {
        if face.neighbor.is_none() && matches!(face.boundary, Boundary::Wall) {
            return self.wall_gradient(face, workspace);
        }
        if self.extrapolated_outflow(face) {
            return [g.raw[1], g.raw[2], g.raw[3], g.raw[5]];
        }
        if let Some(n) = face.neighbor {
            let point = |i: usize| axisymmetry::GradientPoint {
                state: workspace.primitive[i],
                temperature: workspace.temperature_mean[i],
                center: self.mesh.cells[i].center,
                gradient: workspace.gradient[i].regular,
                coordinates: axisymmetry::moment_coordinates(&self.mesh.cells[i]),
            };
            let mut gradient = axisymmetry::face_gradient(
                self.gas,
                point(face.owner),
                point(n),
                face.center,
                self.viscous_velocity(face, workspace),
            );
            if self.mesh.cells[face.owner].radial_moments.is_some() {
                gradient[0] = axial::face_gradient(self, workspace, face, n);
            }
            return gradient;
        }
        let (other, location) = self.sample(face, face.owner, &workspace.primitive);
        let delta = sub(location, self.mesh.cells[face.owner].center);
        let temperatures = [
            workspace.temperature_mean[face.owner],
            Self::sample_temperature(
                face,
                face.owner,
                &workspace.temperature_mean,
                self.gas.temperature(other),
            ),
        ];
        let mut gradient = thermal_gradient(q, other, temperatures, g.raw, delta);
        if matches!(face.boundary, Boundary::Outflow) {
            for g in &mut gradient {
                *g = sub(*g, scale(face.normal, dot(*g, face.normal)));
            }
        }
        gradient
    }

    fn spectral_radius(&self, face: &Face, workspace: &mut Workspace) {
        for id in [Some(face.owner), face.neighbor].into_iter().flatten() {
            let q = workspace.primitive[id];
            let distance = dot(sub(face.center, self.mesh.cells[id].center), face.normal).abs();
            let nu = (4.0 * self.gas.viscosity / 3.0).max(self.gas.conductivity / self.gas.cp())
                / q.density;
            let speed =
                dot(q.velocity, face.normal).abs() + self.gas.sound_speed(q) + 2.0 * nu / distance;
            workspace.time_step[id] += face.area * speed;
        }
    }
}

#[cfg(test)]
mod tests;
