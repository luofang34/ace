use super::{FlowOperator, Workspace, axisymmetry, reconstruction};
use crate::CfdError;
use crate::mesh::{Boundary, Cell, Mesh};

mod moments;
mod stencil;
pub(super) use stencil::{Stencil, stencils};

pub(super) fn coefficients(stencil: &Stencil, value: impl Fn(usize) -> f64, i: usize) -> [f64; 8] {
    let mut result = [0.0; 8];
    for (n, weight) in &stencil.neighbors {
        let difference = value(*n) - value(i);
        for (c, w) in result.iter_mut().zip(weight) {
            *c += w * difference;
        }
    }
    result
}

pub(super) fn change(cell: &Cell, c: [f64; 8], point: [f64; 3]) -> f64 {
    // Even Cartesian quartics include x³, x²y² and x⁴. A quadratic fit in (x,y²)
    // alone aliases their axial curvature into a spurious radial pressure force.
    // Center every monomial by its volume mean to preserve the supplied cell mean.
    let [x, z, _] = axisymmetry::deltas(cell, point)[0];
    let covariance = cell
        .radial_moments
        .as_ref()
        .map_or([0.0; 3], |m| m.scalar_covariance);
    c[0] * x
        + c[1] * z
        + 0.5 * c[2] * (x * x - covariance[0])
        + c[3] * (x * z - covariance[1])
        + 0.5 * c[4] * (z * z - covariance[2])
        + c[5]
            * (x.powi(3)
                - cell
                    .radial_moments
                    .as_ref()
                    .map_or(0.0, |m| m.scalar_third_x))
            / 6.0
        + 0.5
            * c[6]
            * (x * x * z
                - cell
                    .radial_moments
                    .as_ref()
                    .map_or(0.0, |m| m.scalar_mixed_xxz))
        + c[7]
            * (x.powi(4)
                - cell
                    .radial_moments
                    .as_ref()
                    .map_or(0.0, |m| m.scalar_fourth_x))
            / 24.0
}

pub(super) fn planar_mean(cell: &Cell, mean: f64, c: [f64; 8]) -> f64 {
    // Cylindrical pressure sources use dA, whereas stored means use y*dA.
    // Integrate the same polynomial as the faces under the source's measure.
    let Some(m) = &cell.radial_moments else {
        return mean
            + c[0] * (cell.planar_center[0] - cell.center[0])
            + c[1] * (cell.planar_center[1] - cell.center[1]);
    };
    let origin = axisymmetry::moment_coordinates(cell)[0];
    let x = cell.planar_center[0] - origin[0];
    let z = (m.planar_y_squared - origin[1]) / (2.0 * cell.center[1]);
    let difference: [f64; 3] =
        std::array::from_fn(|k| m.planar_covariance[k] - m.scalar_covariance[k]);
    mean + c[0] * x
        + c[1] * z
        + 0.5 * c[2] * (x * x + difference[0])
        + c[3] * (x * z + difference[1])
        + 0.5 * c[4] * (z * z + difference[2])
        + c[5]
            * (x.powi(3) + 3.0 * x * m.planar_covariance[0] + m.planar_third_x - m.scalar_third_x)
            / 6.0
        + 0.5
            * c[6]
            * (x * x * z
                + z * m.planar_covariance[0]
                + 2.0 * x * m.planar_covariance[1]
                + m.planar_mixed_xxz
                - m.scalar_mixed_xxz)
        + c[7]
            * (x.powi(4)
                + 6.0 * x * x * m.planar_covariance[0]
                + 4.0 * x * m.planar_third_x
                + m.planar_fourth_x
                - m.scalar_fourth_x)
            / 24.0
}

pub(super) fn fit(op: &FlowOperator, ws: &Workspace, i: usize) -> [f64; 8] {
    let sensor = ws.shock_sensor[i];
    let g = reconstruction::flatten(ws.gradient[i], sensor).limited[4];
    let stencil = &op.pressure_stencils[i];
    if stencil.neighbors.is_empty() {
        return [g[0], g[1], 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }
    // A polynomial must not sample across a shock outside its own detection support.
    let support = stencil
        .neighbors
        .iter()
        .map(|(n, _)| ws.shock_sensor[*n].max(ws.pressure_defect[*n]))
        .fold(sensor.max(ws.pressure_defect[i]), f64::max);
    // Both candidates must reproduce an affine field independently of the
    // blend weight. Flattening the fallback again would attenuate its slope
    // even when the polynomial and linear fit agree exactly.
    let raw = ws.pressure[i];
    let mut c = raw.map(|v| (1.0 - support) * v);
    c[0] += support * g[0];
    c[1] += support * g[1];
    let theta = limiter(op, ws, i, c, support);
    c.map(|v| v * theta)
}

fn bounds(op: &FlowOperator, ws: &Workspace, i: usize, c: [f64; 8], support: f64) -> [f64; 2] {
    let own = ws.primitive[i].pressure;
    let mut bounds = [own, own];
    let mut include = |value: f64| {
        bounds[0] = bounds[0].min(value);
        bounds[1] = bounds[1].max(value);
    };
    // Extrema need a surrounding patch, beyond the sparse interpolation nodes,
    // so a valid affine field is not clipped beside the wall.
    for (n, _) in &op.pressure_stencils[i].comparisons {
        include(ws.primitive[*n].pressure);
    }
    let cell = &op.mesh.cells[i];
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if face.neighbor.is_some() || op.extrapolated_outflow(face) {
            continue;
        }
        if matches!(face.boundary, Boundary::Axis) {
            for p in face.integration_points() {
                // A smooth even field may reach its extremum at the axis.
                // A troubled polynomial cannot expand its own admissible range.
                include(own + (1.0 - support) * change(cell, c, p.center));
            }
        } else {
            include(op.sample(face, i, &ws.primitive).0.pressure);
        }
    }
    bounds
}

fn limiter(op: &FlowOperator, ws: &Workspace, i: usize, c: [f64; 8], support: f64) -> f64 {
    let cell = &op.mesh.cells[i];
    let mean = ws.primitive[i].pressure;
    let [lower, upper] = bounds(op, ws, i, c, support);
    let mut fraction = 1.0_f64;
    let mut positive = 1.0_f64;
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        let impose = face.neighbor.is_none()
            && (op.extrapolated_outflow(face)
                || matches!(face.boundary, Boundary::Axis | Boundary::Wall));
        for p in face.integration_points() {
            let delta = change(cell, c, p.center);
            if !impose && delta.abs() > 1e-14 * mean.abs().max(1.0) {
                let bound = if delta > 0.0 {
                    upper - mean
                } else {
                    lower - mean
                };
                fraction = fraction.min((bound / delta).clamp(0.0, 1.0));
            }
            if delta < 0.0 {
                positive = positive.min(0.99 * mean / -delta);
            }
        }
    }
    if let Some(m) = &cell.radial_moments {
        for p in m.reconstruction_points {
            let delta = change(cell, c, p);
            if delta < 0.0 {
                positive = positive.min(0.99 * mean / -delta);
            }
        }
    }
    positive.min(1.0 - support * (1.0 - fraction))
}

pub(super) fn model_defect(stencil: &Stencil, ws: &Workspace, i: usize, c: [f64; 8]) -> f64 {
    let own = ws.primitive[i].pressure;
    let mut range = [own, own];
    let mut defect = 0.0_f64;
    for (n, row) in &stencil.comparisons {
        let value = ws.primitive[*n].pressure;
        range[0] = range[0].min(value);
        range[1] = range[1].max(value);
        let change: f64 = c.iter().zip(row).map(|(a, b)| a * b).sum();
        defect = defect.max((value - own - change).abs());
    }
    // Held-out cell means distinguish resolved extrema from jumps, independently
    // of jump amplitude. A maximum avoids weighting an axis row by sample count.
    let variation = (range[1] - range[0]).max(64.0 * f64::EPSILON * own.abs());
    (defect / variation).powi(2).min(1.0)
}

#[cfg(test)]
mod tests;
