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
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if op.extrapolated_outflow(face) {
            continue;
        }
        let (q, location) = op.sample(face, i, states);
        let values = axisymmetry::components(op.gas, q, location[1]);
        for k in 0..5 {
            lower[k] = lower[k].min(values[k]);
            upper[k] = upper[k].max(values[k]);
        }
        // Face quadrature in cell-moment coordinates extends beyond the polygon
        // of face-neighbour means near the axis. The second ring includes the
        // corner neighbours needed to bound an affine field without clipping it.
        if let Some(n) = face
            .neighbor
            .map(|n| if face.owner == i { n } else { face.owner })
        {
            for id in op.mesh.cells[n].faces {
                let other = &op.mesh.faces[id];
                if let Some(m) = other
                    .neighbor
                    .map(|m| if other.owner == n { m } else { other.owner })
                {
                    let values =
                        axisymmetry::components(op.gas, states[m], op.mesh.cells[m].center[1]);
                    for k in 0..5 {
                        lower[k] = lower[k].min(values[k]);
                        upper[k] = upper[k].max(values[k]);
                    }
                }
            }
        }
        if face.neighbor.is_none() && matches!(face.boundary, Boundary::Axis) {
            // Even fields have a physical extremum at the axis. Their boundary
            // values follow from the moment fit, not a copied cell mean.
            for p in face.integration_points() {
                let d = axisymmetry::deltas(cell, p.center);
                for k in 0..5 {
                    let value = own[k] + dot(gradient[k], d[usize::from(k == 2)]);
                    lower[k] = lower[k].min(value);
                    upper[k] = upper[k].max(value);
                }
            }
        }
    }
    (lower, upper)
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
