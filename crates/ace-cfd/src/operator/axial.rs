use super::{FlowOperator, Workspace, axisymmetry, pressure, quadratic};
use crate::mesh::{Cell, Face};
use crate::vector::{Vector, add, dot, scale, sub};

pub(super) fn fit(op: &FlowOperator, ws: &Workspace, i: usize) -> [f64; 8] {
    let compact = &op.pressure_stencils[i];
    if !compact.neighbors.is_empty() {
        // A common axial interpolation support prevents unfitted axial variation
        // from biasing neighboring angular rows by different amounts near r=0.
        return pressure::coefficients(compact, |n| ws.primitive[n].velocity[0], i);
    }
    let stencil = &op.quadratic_stencils[i];
    if stencil.neighbors.is_empty() {
        let g = ws.gradient[i].regular[1];
        return [g[0], g[1], 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }
    let mut c = [0.0; 8];
    c[..5].copy_from_slice(&quadratic::coefficients(
        stencil,
        |n| ws.primitive[n].velocity[0],
        i,
    ));
    c
}

pub(super) fn change(cell: &Cell, c: [f64; 8], point: Vector) -> f64 {
    pressure::change(cell, c, point)
}

fn gradient(cell: &Cell, c: [f64; 8], point: Vector) -> Vector {
    let [x, z, _] = axisymmetry::deltas(cell, point)[0];
    [
        c[0] + c[2] * x + c[3] * z + 0.5 * c[5] * x * x + c[6] * x * z + c[7] * x.powi(3) / 6.0,
        (c[1] + c[3] * x + c[4] * z + 0.5 * c[6] * x * x) * point[1] / cell.center[1],
        0.0,
    ]
}

pub(super) fn hoop_stress(op: &FlowOperator, ws: &Workspace, i: usize) -> f64 {
    let cell = &op.mesh.cells[i];
    let mut g = ws.gradient[i].regular;
    if let Some(m) = &cell.radial_moments {
        let origin = axisymmetry::moment_coordinates(cell)[0];
        let c = ws.axial[i];
        let x = cell.planar_center[0] - origin[0];
        let z = (m.planar_y_squared - origin[1]) / (2.0 * cell.center[1]);
        // The cylindrical source integrates a planar mean of du/dx. Reusing
        // its volume-centred value leaves curvature error amplified by 1/r.
        g[1][0] = c[0]
            + c[2] * x
            + c[3] * z
            + 0.5 * c[5] * (x * x + m.planar_covariance[0])
            + c[6] * (x * z + m.planar_covariance[1])
            + c[7] * (x.powi(3) + 3.0 * x * m.planar_covariance[0] + m.planar_third_x) / 6.0;
    }
    axisymmetry::hoop_stress(op.gas, ws.primitive[i], cell, g)
}

pub(super) fn face_gradient(op: &FlowOperator, ws: &Workspace, face: &Face, n: usize) -> Vector {
    let i = face.owner;
    let left = &op.mesh.cells[i];
    let right = &op.mesh.cells[n];
    let average = scale(
        add(
            gradient(left, ws.axial[i], face.center),
            gradient(right, ws.axial[n], face.center),
        ),
        0.5,
    );
    let l = ws.primitive[i].velocity[0] + change(left, ws.axial[i], face.center);
    let r = ws.primitive[n].velocity[0] + change(right, ws.axial[n], face.center);
    let delta = sub(right.center, left.center);
    // Penalize the reconstructed jump, which vanishes for a reproduced polynomial.
    add(average, scale(delta, (r - l) / dot(delta, delta)))
}
