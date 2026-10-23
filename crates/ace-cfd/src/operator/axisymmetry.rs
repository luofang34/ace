use super::FlowOperator;
use crate::mesh::{Boundary, Cell, Face};
use crate::vector::{Vector, add, dot, scale, sub};
use crate::{IdealGas, Primitive};

pub(super) fn recover_mean_primitives(op: &FlowOperator, workspace: &mut super::Workspace) {
    for (i, cell) in op.mesh.cells.iter().enumerate() {
        let Some(moments) = cell.radial_moments else {
            continue;
        };
        let [xx, xy, yy] = moments.covariance;
        let density_gradient = workspace.gradient[i].raw[0];
        let transverse_density = workspace.primitive[i].density
            + density_gradient[0] * xy / cell.center[1]
            + density_gradient[1] * (moments.transverse_y_squared - moment_coordinates(cell)[0][1])
                / (2.0 * cell.center[1]);
        let variance: f64 = workspace.gradient[i].raw[1..4]
            .iter()
            .map(|g| g[0] * g[0] * xx + 2.0 * g[0] * g[1] * xy + g[1] * g[1] * yy)
            .sum();
        let q = &mut workspace.primitive[i];
        // Conservative momentum stores a density-weighted velocity. Reconstruction
        // uses volume means, so remove the leading density-velocity covariance.
        for k in [0, 2] {
            let g = workspace.gradient[i].raw[k + 1];
            let covariance = density_gradient[0] * g[0] * xx
                + (density_gradient[0] * g[1] + density_gradient[1] * g[0]) * xy
                + density_gradient[1] * g[1] * yy;
            q.velocity[k] -= covariance / q.density;
        }
        // The mean kinetic energy includes the velocity variance inside a cell.
        // Treating it as thermal energy creates an artificial pressure gradient.
        // Bound the reconstruction correction without altering the conserved state.
        let correction = 0.5 * (op.gas.gamma - 1.0) * q.density * variance.max(0.0);
        q.pressure -= correction.min(0.99 * q.pressure);
        let transverse_gradient = workspace.gradient[i].regular[2];
        let [transverse_xx, transverse_xy, transverse_yy] = moments.transverse_covariance;
        let transverse_covariance = density_gradient[0] * transverse_gradient[0] * transverse_xx
            + (density_gradient[0] * transverse_gradient[1]
                + density_gradient[1] * transverse_gradient[0])
                * transverse_xy
            + density_gradient[1] * transverse_gradient[1] * transverse_yy;
        // Odd radial momentum carries a y weight. Correct both the weighted
        // density and its leading covariance with the even field v_y/y.
        q.velocity[1] = (workspace.mean_state[i][2] - cell.center[1] * transverse_covariance)
            / transverse_density.max(0.01 * q.density);
        let temperature = op.gas.temperature(*q);
        let thermal_gradient = workspace.gradient[i].raw[5];
        let covariance = density_gradient[0] * thermal_gradient[0] * xx
            + (density_gradient[0] * thermal_gradient[1]
                + density_gradient[1] * thermal_gradient[0])
                * xy
            + density_gradient[1] * thermal_gradient[1] * yy;
        // Mean pressure divided by mean density gives a density-weighted
        // temperature. The viscous moment fit requires its volume mean.
        workspace.temperature_mean[i] =
            temperature - (covariance / q.density).min(0.99 * temperature);
    }
}

// Smooth axisymmetric scalars and v_r/r are even in r. Fitting in r² preserves
// their leading curvature, which the cylindrical 1/r divergence amplifies.
pub(super) fn components(gas: IdealGas, q: Primitive, radius: f64) -> [f64; 6] {
    [
        q.density,
        q.velocity[0],
        q.velocity[1] / radius,
        q.velocity[2],
        q.pressure,
        gas.temperature(q),
    ]
}

pub(super) fn hoop_stress(gas: IdealGas, q: Primitive, cell: &Cell, gradient: [Vector; 6]) -> f64 {
    let radius = cell.center[1];
    let [x, yy_weighted] = moment_coordinates(cell)[1];
    let yy_planar = cell
        .radial_moments
        .map_or(radius * cell.volume / cell.planar_area, |m| {
            m.planar_y_squared
        });
    // The curvature source is a planar-area integral. Radial momentum stores a
    // y-weighted mean of a=v/y; using that mean directly leaves an artificial 1/y force.
    let a = q.velocity[1] / radius
        + gradient[2][0] * (cell.planar_center[0] - x)
        + gradient[2][1] * (yy_planar - yy_weighted) / (2.0 * radius);
    (2.0 / 3.0) * gas.viscosity * (a - gradient[1][0] - gradient[2][1] * yy_planar / radius)
}

pub(super) fn moment_coordinates(cell: &Cell) -> [[f64; 2]; 2] {
    let [x, y, _] = cell.center;
    cell.radial_moments.map_or([[x, y * y]; 2], |m| {
        [
            [x, y * y + m.covariance[2]],
            [x + m.covariance[1] / y, m.transverse_y_squared],
        ]
    })
}

pub(super) fn deltas(cell: &Cell, point: Vector) -> [Vector; 2] {
    moment_coordinates(cell).map(|[x, yy]| {
        [
            point[0] - x,
            (point[1] * point[1] - yy) / (2.0 * cell.center[1]),
            0.0,
        ]
    })
}

fn sample_coordinates(op: &FlowOperator, face: &Face, i: usize, position: Vector) -> [[f64; 2]; 2] {
    if let Some(n) = face.neighbor {
        return moment_coordinates(&op.mesh.cells[if i == face.owner { n } else { face.owner }]);
    }
    if op.mesh.cells[i].radial_moments.is_none() {
        return [[position[0], position[1] * position[1]]; 2];
    }
    if matches!(face.boundary, Boundary::Wall | Boundary::Inflow) {
        let mut s = [0.0; 5];
        for p in face.integration_points() {
            let [x, y, _] = p.center;
            for (total, v) in s.iter_mut().zip([x, y * y, x * y, y.powi(3), y]) {
                *total += v * p.area;
            }
        }
        return [
            [s[0] / face.area, s[1] / face.area],
            [s[2] / s[4], s[3] / s[4]],
        ];
    }
    let cell = &op.mesh.cells[i];
    moment_coordinates(cell).map(|[x, yy]| {
        [
            x + position[0] - cell.center[0],
            yy + position[1].powi(2) - cell.center[1].powi(2),
        ]
    })
}

pub(super) fn gradient(
    op: &FlowOperator,
    states: &[Primitive],
    temperatures: &[f64],
    i: usize,
) -> [Vector; 6] {
    let cell = &op.mesh.cells[i];
    let mut own = components(op.gas, states[i], cell.center[1]);
    own[5] = temperatures[i];
    let own_coordinates = moment_coordinates(cell);
    let mut matrix = [[0.0; 3]; 2];
    let mut rhs = [[0.0; 2]; 6];
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if op.extrapolated_outflow(face)
            || (face.neighbor.is_none() && matches!(face.boundary, Boundary::Axis))
        {
            continue;
        }
        let (q, position) = op.sample(face, i, states);
        let mut values = components(op.gas, q, position[1]);
        values[5] = FlowOperator::sample_temperature(face, i, temperatures, values[5]);
        let edge = sub(position, cell.center);
        let weight = 1.0 / dot(edge, edge);
        let other_coordinates = sample_coordinates(op, face, i, position);
        let deltas: [[f64; 2]; 2] = std::array::from_fn(|k| {
            [
                other_coordinates[k][0] - own_coordinates[k][0],
                (other_coordinates[k][1] - own_coordinates[k][1]) / (2.0 * cell.center[1]),
            ]
        });
        for (m, d) in matrix.iter_mut().zip(deltas) {
            m[0] += d[0] * d[0] * weight;
            m[1] += d[0] * d[1] * weight;
            m[2] += d[1] * d[1] * weight;
        }
        for k in 0..6 {
            let d = deltas[usize::from(k == 2)];
            rhs[k][0] += (values[k] - own[k]) * d[0] * weight;
            rhs[k][1] += (values[k] - own[k]) * d[1] * weight;
        }
    }
    std::array::from_fn(|k| {
        let [a, b, c] = matrix[usize::from(k == 2)];
        let determinant = a * c - b * b;
        let [x, y] = rhs[k];
        [
            (c * x - b * y) / determinant,
            (a * y - b * x) / determinant,
            0.0,
        ]
    })
}

pub(super) fn physical(q: Primitive, radius: f64, mut g: [Vector; 6]) -> [Vector; 6] {
    g[2] = [
        radius * g[2][0],
        q.velocity[1] / radius + radius * g[2][1],
        0.0,
    ];
    g
}

pub(super) fn velocity(q: Primitive, cell: &Cell, face: Vector, g: [Vector; 6]) -> Vector {
    let center = cell.center;
    let d = deltas(cell, face);
    [
        q.velocity[0] + dot(g[1], d[0]),
        (q.velocity[1] / center[1] + dot(g[2], d[1])) * face[1],
        q.velocity[2] + dot(g[3], d[0]),
    ]
}

pub(super) struct GradientPoint {
    pub state: Primitive,
    pub temperature: f64,
    pub center: Vector,
    pub gradient: [Vector; 6],
    pub coordinates: [[f64; 2]; 2],
}

pub(super) fn face_gradient(
    gas: IdealGas,
    left: GradientPoint,
    right: GradientPoint,
    face: Vector,
    velocity: Vector,
) -> [Vector; 4] {
    let mut a = components(gas, left.state, left.center[1]);
    let mut b = components(gas, right.state, right.center[1]);
    a[5] = left.temperature;
    b[5] = right.temperature;
    let midpoint_radius = 0.5 * (left.center[1] + right.center[1]);
    std::array::from_fn(|k| {
        let field = if k == 3 { 5 } else { k + 1 };
        let component = usize::from(field == 2);
        let delta = [
            right.coordinates[component][0] - left.coordinates[component][0],
            (right.coordinates[component][1] - left.coordinates[component][1])
                / (2.0 * midpoint_radius),
            0.0,
        ];
        let l = left.gradient[field];
        let r = right.gradient[field];
        let g = [
            0.5 * (l[0] + r[0]),
            0.5 * midpoint_radius * (l[1] / left.center[1] + r[1] / right.center[1]),
            0.0,
        ];
        let mut corrected = add(
            g,
            scale(
                delta,
                (b[field] - a[field] - dot(g, delta)) / dot(delta, delta),
            ),
        );
        corrected[1] *= face[1] / midpoint_radius;
        if k == 1 {
            [
                face[1] * corrected[0],
                velocity[1] / face[1] + face[1] * corrected[1],
                0.0,
            ]
        } else {
            corrected
        }
    })
}

#[cfg(test)]
mod manufactured;
#[cfg(test)]
mod tests;
