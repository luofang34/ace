use super::*;
use crate::mesh::RadialMoments;

impl FlowOperator {
    pub(super) fn curved_wall_pressure(&self, face: &Face, states: &[Primitive]) -> Option<f64> {
        let first = self.mesh.cells[face.owner].radial_moments?.scalar[0];
        let n = self.wall_normal_neighbor(face)?;
        let other = self.mesh.cells[n].radial_moments?.scalar[0];
        let own = states[face.owner].pressure;
        // A no-slip wall does not prescribe zero normal pressure gradient.
        // Extrapolate the same cell means used by the interior pressure fit.
        let pressure = own + first * (own - states[n].pressure) / (other - first);
        Some(pressure.max(0.01 * own))
    }

    pub(super) fn curved_wall_gradient(
        &self,
        face: &Face,
        workspace: &Workspace,
        first: RadialMoments,
    ) -> [Vector; 4] {
        let inward = scale(face.normal, -1.0);
        let own = workspace.primitive[face.owner];
        let values = |i: usize| {
            let u = workspace.mean_state[i];
            // Z vanishes at an isothermal no-slip wall. Its derivative there is
            // rho_wall * dT/dn / T_wall; kinetic energy has zero first derivative.
            let cv = self.gas.cp() / self.gas.gamma;
            [
                u[1],
                u[2] / self.mesh.cells[i].center[1],
                u[3],
                u[4] / (cv * self.wall_temperature) - u[0],
            ]
        };
        let moments = |i: usize| self.mesh.cells[i].radial_moments;
        let a = values(face.owner);
        let next = self.wall_normal_neighbor(face);
        let m = first;
        let mut slope = std::array::from_fn(|k| a[k] / coordinate(m, k)[0]);
        let mut density = own.density;
        if let Some((n, other)) = next.and_then(|n| moments(n).map(|m| (n, m))) {
            let b = values(n);
            slope = std::array::from_fn(|k| {
                let [m1, m2] = coordinate(m, k);
                let [n1, n2] = coordinate(other, k);
                (n2 * a[k] - m2 * b[k]) / (m1 * n2 - m2 * n1)
            });
            density = (other.scalar[0] * own.density
                - m.scalar[0] * workspace.primitive[n].density)
                / (other.scalar[0] - m.scalar[0]);
        }
        // As for convective reconstruction, limit extrapolation to a positive state.
        density = density.max(0.01 * own.density);
        slope[1] *= face.center[1];
        slope[3] *= self.wall_temperature;
        slope.map(|d| scale(inward, d / density))
    }
}

fn coordinate(moments: RadialMoments, component: usize) -> [f64; 2] {
    if component == 1 {
        moments.transverse
    } else {
        moments.scalar
    }
}
