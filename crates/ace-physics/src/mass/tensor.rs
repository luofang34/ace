use super::ItemState;
use crate::bindings::scalar;
use crate::{PhysicsError, PhysicsResult};
use ace_model::{Inertia, Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn resolve(
    inertia: &Inertia,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<[[f64; 3]; 3]> {
    let xx = scalar(&inertia.xx, values, Unit::Inertia)?;
    let yy = scalar(&inertia.yy, values, Unit::Inertia)?;
    let zz = scalar(&inertia.zz, values, Unit::Inertia)?;
    let xy = scalar(&inertia.xy, values, Unit::Inertia)?;
    let xz = scalar(&inertia.xz, values, Unit::Inertia)?;
    let yz = scalar(&inertia.yz, values, Unit::Inertia)?;
    let matrix = [[xx, xy, xz], [xy, yy, yz], [xz, yz, zz]];
    // A physical inertia is trace(C)I - C for a positive semidefinite second-moment matrix C.
    let half_trace = (xx + yy + zz) * 0.5;
    let mut moment = matrix.map(|row| row.map(|value| -value));
    for (axis, row) in moment.iter_mut().enumerate() {
        row[axis] += half_trace;
    }
    if !positive_semidefinite(moment) {
        return Err(PhysicsError::invalid(
            "mass.inertia",
            "tensor cannot represent a nonnegative mass distribution",
        ));
    }
    Ok(matrix)
}

fn positive_semidefinite(matrix: [[f64; 3]; 3]) -> bool {
    let scale = matrix
        .iter()
        .flatten()
        .fold(1.0_f64, |scale, value| scale.max(value.abs()));
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return false;
    }
    let m = matrix.map(|row| row.map(|value| value / scale));
    if (0..3).any(|axis| m[axis][axis] < -1e-12) {
        return false;
    }
    for i in 0..3 {
        for j in (i + 1)..3 {
            if m[i][i] * m[j][j] - m[i][j].powi(2) < -1e-12 {
                return false;
            }
        }
    }
    let determinant = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    determinant >= -1e-12
}

pub(super) fn aggregate(
    items: &BTreeMap<String, ItemState>,
    cg: [f64; 3],
) -> PhysicsResult<Option<[[f64; 3]; 3]>> {
    let mut tensor = [[0.0; 3]; 3];
    for item in items.values().filter(|item| item.mass_kg > 0.0) {
        let Some(intrinsic) = item.inertia_kg_m2 else {
            return Ok(None);
        };
        let offset = [
            item.cg_m[0] - cg[0],
            item.cg_m[1] - cg[1],
            item.cg_m[2] - cg[2],
        ];
        let squared_distance = offset.iter().map(|value| value.powi(2)).sum::<f64>();
        for (i, row) in tensor.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value += intrinsic[i][j]
                    + item.mass_kg
                        * (if i == j { squared_distance } else { 0.0 } - offset[i] * offset[j]);
            }
        }
    }
    if tensor.iter().flatten().any(|value| !value.is_finite()) {
        return Err(PhysicsError::invalid(
            "mass.inertia",
            "non-finite aggregate inertia",
        ));
    }
    Ok(Some(tensor))
}
