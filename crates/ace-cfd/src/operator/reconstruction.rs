use crate::vector::{Vector, add, dot, scale, sub};
use crate::{FlowOperator, IdealGas, Primitive};

mod curved;
pub(super) use curved::reconstruct as reconstruct_curved;
mod shock;
pub(super) use shock::limit_shock_gradients;

#[derive(Clone, Copy, Default)]
pub(super) struct Gradient {
    pub raw: [Vector; 6],
    pub regular: [Vector; 6],
    pub limited: [Vector; 5],
}

pub(super) fn flatten(mut gradient: Gradient, sensor: f64) -> Gradient {
    let pressure = gradient.raw[4];
    let square = dot(pressure, pressure);
    // Face fluxes and the cylindrical pressure source must use the same polynomial.
    // Retain the physical face radius when suppressing the shock-normal slopes.
    if square > 0.0 {
        for slope in &mut gradient.limited {
            *slope = sub(
                *slope,
                scale(pressure, sensor * dot(*slope, pressure) / square),
            );
        }
    }
    gradient
}

pub(super) fn gradients(
    op: &FlowOperator,
    states: &[Primitive],
    temperatures: &[f64],
    output: &mut [Gradient],
) {
    for (i, cell) in op.mesh.cells.iter().enumerate() {
        let mut own = components(op.gas, states[i]);
        own[5] = temperatures[i];
        let mut matrix = [0.0; 3];
        let mut rhs = [[0.0; 2]; 6];
        let mut lower = own;
        let mut upper = own;
        for id in cell.faces {
            let face = &op.mesh.faces[id];
            if op.extrapolated_outflow(face) {
                continue;
            }
            let (other, position) = op.sample(face, i, states);
            let mut values = components(op.gas, other);
            values[5] = FlowOperator::sample_temperature(face, i, temperatures, values[5]);
            let d = sub(position, cell.center);
            let weight = 1.0 / dot(d, d);
            matrix[0] += d[0] * d[0] * weight;
            matrix[1] += d[0] * d[1] * weight;
            matrix[2] += d[1] * d[1] * weight;
            for k in 0..6 {
                for (a, coordinate) in d.iter().take(2).enumerate() {
                    rhs[k][a] += (values[k] - own[k]) * coordinate * weight;
                }
                lower[k] = lower[k].min(values[k]);
                upper[k] = upper[k].max(values[k]);
            }
        }
        let [a, b, c] = matrix;
        let determinant = a * c - b * b;
        let raw = rhs.map(|[x, y]| {
            [
                (c * x - b * y) / determinant,
                (a * y - b * x) / determinant,
                0.0,
            ]
        });
        let regular = super::axisymmetry::gradient(op, states, temperatures, i);
        output[i] = Gradient {
            raw: super::axisymmetry::physical(states[i], cell.center[1], regular),
            regular,
            limited: if cell.radial_moments.is_some() {
                curved::limit(op, states, i, regular)
            } else {
                limit(op, cell, own, lower, upper, std::array::from_fn(|k| raw[k]))
            },
        };
    }
}

fn limit(
    op: &FlowOperator,
    cell: &crate::mesh::Cell,
    own: [f64; 6],
    lower: [f64; 6],
    upper: [f64; 6],
    raw: [Vector; 5],
) -> [Vector; 5] {
    let mut limiter = [1.0_f64; 5];
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        // Imposed wall/axis fluxes do not use a reconstructed face state.
        if face.neighbor.is_none()
            && (op.extrapolated_outflow(face)
                || matches!(
                    face.boundary,
                    crate::mesh::Boundary::Wall | crate::mesh::Boundary::Axis
                ))
        {
            continue;
        }
        let delta = sub(face.center, cell.center);
        for k in 0..5 {
            let change = dot(raw[k], delta);
            let bound = if change > 0.0 {
                upper[k] - own[k]
            } else {
                lower[k] - own[k]
            };
            if change.abs() > 1.0e-14 * own[k].abs().max(1.0) {
                // Unequal centroid spacing beside the axis must not limit an affine field.
                limiter[k] = limiter[k].min((bound / change).clamp(0.0, 1.0));
                if (k == 0 || k == 4) && change < 0.0 {
                    limiter[k] = limiter[k].min(0.99 * own[k] / -change);
                }
            }
        }
    }
    std::array::from_fn(|k| scale(raw[k], limiter[k]))
}

fn components(gas: IdealGas, q: Primitive) -> [f64; 6] {
    [
        q.density,
        q.velocity[0],
        q.velocity[1],
        q.velocity[2],
        q.pressure,
        gas.temperature(q),
    ]
}

pub(super) fn reconstruct(q: Primitive, gradient: Gradient, delta: Vector) -> Primitive {
    let change: [f64; 5] = std::array::from_fn(|k| dot(gradient.limited[k], delta));
    increment(q, change)
}

fn increment(q: Primitive, change: [f64; 5]) -> Primitive {
    let own = q.components();
    // Preserve positive thermodynamics even for boundary traces outside the
    // interior-face limiter constraints, without altering the cell mean.
    let mut fraction = 1.0_f64;
    for k in [0, 4] {
        if change[k] < 0.0 {
            fraction = fraction.min(0.99 * own[k] / -change[k]);
        }
    }
    Primitive::from_components(std::array::from_fn(|k| own[k] + fraction * change[k]))
}

// The edge correction makes the diffusive stencil reproduce linear fields even on skew grids.
pub(super) fn thermal_gradient(
    a: Primitive,
    b: Primitive,
    temperatures: [f64; 2],
    average: [Vector; 6],
    delta: Vector,
) -> [Vector; 4] {
    let difference = [
        b.velocity[0] - a.velocity[0],
        b.velocity[1] - a.velocity[1],
        b.velocity[2] - a.velocity[2],
        temperatures[1] - temperatures[0],
    ];
    std::array::from_fn(|k| {
        let g = average[if k == 3 { 5 } else { k + 1 }];
        add(
            g,
            scale(delta, (difference[k] - dot(g, delta)) / dot(delta, delta)),
        )
    })
}

#[cfg(test)]
mod tests;
