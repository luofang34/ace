use super::{FlowOperator, Gradient, Primitive, increment};
use crate::mesh::{Boundary, Cell};
use crate::operator::axisymmetry;
use crate::vector::{Vector, dot, scale};

pub(super) fn limit(
    op: &FlowOperator,
    states: &[Primitive],
    i: usize,
    gradient: [Vector; 6],
) -> [Vector; 5] {
    let cell = &op.mesh.cells[i];
    let own = axisymmetry::components(op.gas, states[i], cell.center[1]);
    let (lower, upper) = bounds(op, states, i, gradient);
    limit_slopes(
        op,
        cell,
        own,
        lower,
        upper,
        std::array::from_fn(|k| gradient[k]),
    )
}

pub(super) fn limit_projected(
    op: &FlowOperator,
    states: &[Primitive],
    i: usize,
    gradient: Gradient,
) -> [Vector; 5] {
    let cell = &op.mesh.cells[i];
    let own = axisymmetry::components(op.gas, states[i], cell.center[1]);
    // Projection changes the slopes, not the admissible sample range.
    let (lower, upper) = bounds(op, states, i, gradient.regular);
    limit_slopes(op, cell, own, lower, upper, gradient.limited)
}

fn limit_slopes(
    op: &FlowOperator,
    cell: &Cell,
    own: [f64; 6],
    lower: [f64; 6],
    upper: [f64; 6],
    gradient: [Vector; 5],
) -> [Vector; 5] {
    let mut factors = [1.0_f64; 5];
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if face.neighbor.is_none()
            && (op.extrapolated_outflow(face)
                || matches!(face.boundary, Boundary::Wall | Boundary::Axis))
        {
            continue;
        }
        for p in face.integration_points() {
            let d = axisymmetry::deltas(cell, p.center);
            for k in 0..5 {
                let change = dot(gradient[k], d[usize::from(k == 2)]);
                let bound = if change > 0.0 {
                    upper[k] - own[k]
                } else {
                    lower[k] - own[k]
                };
                if change.abs() > 1e-14 * own[k].abs().max(1.0) {
                    factors[k] = factors[k].min((bound / change).clamp(0.0, 1.0));
                    if (k == 0 || k == 4) && change < 0.0 {
                        factors[k] = factors[k].min(0.99 * own[k] / -change);
                    }
                }
            }
        }
    }
    std::array::from_fn(|k| scale(gradient[k], factors[k]))
}

fn bounds(
    op: &FlowOperator,
    states: &[Primitive],
    i: usize,
    gradient: [Vector; 6],
) -> ([f64; 6], [f64; 6]) {
    let cell = &op.mesh.cells[i];
    let own = axisymmetry::components(op.gas, states[i], cell.center[1]);
    let mut lower = own;
    let mut upper = own;
    let mut include = |values: [f64; 6]| {
        for k in 0..5 {
            lower[k] = lower[k].min(values[k]);
            upper[k] = upper[k].max(values[k]);
        }
    };
    if let Some(value) = axis_sample(op, states, i, gradient, cell.center[1]) {
        include(value);
    }
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if op.extrapolated_outflow(face) {
            continue;
        }
        let (q, location) = op.sample(face, i, states);
        let values = axisymmetry::components(op.gas, q, location[1]);
        include(values);
        if face.neighbor.is_none() && matches!(face.boundary, Boundary::Axis) {
            // Thermodynamic slopes feed the shock pressure/source balance.
            // Keep that closure independent of the no-slip velocity trace bounds.
            for p in face.integration_points() {
                let d = axisymmetry::deltas(cell, p.center)[0];
                let mut value = own;
                for k in [0, 4] {
                    value[k] += dot(gradient[k], d);
                }
                include(value);
            }
        }
        // Face quadrature in cell-moment coordinates extends beyond the polygon
        // of face-neighbour means near the axis. The second ring includes the
        // corner neighbours needed to bound an affine field without clipping it.
        if let Some(n) = face
            .neighbor
            .map(|n| if face.owner == i { n } else { face.owner })
        {
            if let Some(value) = axis_sample(op, states, n, gradient, cell.center[1]) {
                include(value);
            }
            for id in op.mesh.cells[n].faces {
                let other = &op.mesh.faces[id];
                if let Some(m) = other
                    .neighbor
                    .map(|m| if other.owner == n { m } else { other.owner })
                {
                    let values =
                        axisymmetry::components(op.gas, states[m], op.mesh.cells[m].center[1]);
                    include(values);
                    if let Some(value) = axis_sample(op, states, m, gradient, cell.center[1]) {
                        include(value);
                    }
                }
            }
        }
    }
    (lower, upper)
}

fn axis_sample(
    op: &FlowOperator,
    states: &[Primitive],
    i: usize,
    gradient: [Vector; 6],
    radius: f64,
) -> Option<[f64; 6]> {
    let cell = &op.mesh.cells[i];
    if !cell.faces.iter().any(|id| {
        let face = &op.mesh.faces[*id];
        face.neighbor.is_none() && matches!(face.boundary, Boundary::Axis)
    }) {
        return None;
    }
    let values = axisymmetry::components(op.gas, states[i], cell.center[1]);
    let coordinates = axisymmetry::moment_coordinates(cell);
    // Axis extrema extend the angular range at existing axial sample locations.
    // Extrapolating along an entire axis face can import an unresolved normal
    // curvature overshoot into the range only for the axis-adjacent row.
    Some(std::array::from_fn(|k| {
        if (1..=3).contains(&k) {
            values[k] - gradient[k][1] * coordinates[usize::from(k == 2)][1] / (2.0 * radius)
        } else {
            values[k]
        }
    }))
}

pub(in crate::operator) fn reconstruct(
    q: Primitive,
    gradient: Gradient,
    cell: &Cell,
    point: Vector,
) -> Primitive {
    let d = axisymmetry::deltas(cell, point);
    let mut change = std::array::from_fn(|k| dot(gradient.limited[k], d[0]));
    change[2] = (q.velocity[1] / cell.center[1] + dot(gradient.limited[2], d[1])) * point[1]
        - q.velocity[1];
    increment(q, change)
}

#[cfg(test)]
mod tests;
