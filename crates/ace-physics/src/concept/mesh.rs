use super::SurfacePart;
use crate::{PhysicsError, PhysicsResult, bindings::scalar};
use ace_model::{ConceptShape, Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn generate(
    shape: &ConceptShape,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Vec<SurfacePart>> {
    let m = |b| scalar(b, values, Unit::Meter);
    match shape {
        ConceptShape::Body {
            length,
            width,
            height,
            nose,
            tail,
        } => {
            let dims = [m(length)?, m(width)?, m(height)?, m(nose)?, m(tail)?];
            if dims.iter().any(|v| *v <= 0.0) || dims[3] + dims[4] >= dims[0] {
                return Err(PhysicsError::invalid(
                    "body",
                    "positive dimensions and nose + tail < length required",
                ));
            }
            Ok(vec![body(dims)])
        }
        ConceptShape::Wing {
            span,
            area,
            taper,
            sweep,
            dihedral,
            thickness,
            symmetric,
        } => {
            let dims = [
                m(span)?,
                scalar(area, values, Unit::SquareMeter)?,
                scalar(taper, values, Unit::Ratio)?,
                scalar(sweep, values, Unit::Radian)?,
                scalar(dihedral, values, Unit::Radian)?,
                scalar(thickness, values, Unit::Ratio)?,
            ];
            if dims[0] <= 0.0
                || dims[1] <= 0.0
                || !(0.05..=1.0).contains(&dims[2])
                || dims[3].abs() > 1.22
                || dims[4].abs() > 0.52
                || !(0.01..=0.3).contains(&dims[5])
            {
                return Err(PhysicsError::invalid(
                    "wing",
                    "native shape outside span/area/taper/sweep/dihedral/thickness domain",
                ));
            }
            let mut parts = vec![wing(dims, *symmetric, 1.0)];
            if *symmetric {
                parts.push(wing(dims, true, -1.0));
            }
            Ok(parts)
        }
    }
}

fn body([length, width, height, nose, tail]: [f64; 5]) -> SurfacePart {
    let mut stations = Vec::new();
    for i in 0..=16 {
        let angle = i as f64 / 16.0 * std::f64::consts::FRAC_PI_2;
        stations.push((nose * (1.0 - angle.cos()), angle.sin()));
    }
    for i in 0..=16 {
        let angle = i as f64 / 16.0 * std::f64::consts::FRAC_PI_2;
        stations.push((length - tail + tail * angle.sin(), angle.cos()));
    }
    let vertices = stations
        .iter()
        .flat_map(|(x, r)| {
            (0..32).map(move |j| {
                let angle = j as f64 / 32.0 * std::f64::consts::TAU;
                [
                    *x,
                    r * width * 0.5 * angle.cos(),
                    r * height * 0.5 * angle.sin(),
                ]
            })
        })
        .collect();
    let mut triangles = Vec::new();
    for ring in 0..stations.len() - 1 {
        for j in 0..32 {
            let a = ring * 32 + j;
            let b = ring * 32 + (j + 1) % 32;
            triangles.extend([[a, b, b + 32], [a, b + 32, a + 32]]);
        }
    }
    SurfacePart {
        vertices,
        triangles,
    }
}

fn wing(
    [span, area, taper, sweep, dihedral, thickness]: [f64; 6],
    symmetric: bool,
    sign: f64,
) -> SurfacePart {
    let extent = if symmetric { span * 0.5 } else { span };
    let root = 2.0 * area / (span * (1.0 + taper));
    let mut vertices = Vec::new();
    for (y, chord) in [(0.0, root), (extent, root * taper)] {
        let x = y * sweep.tan();
        let z = y * dihedral.tan();
        vertices.extend([
            [x, sign * y, z],
            [x + chord * 0.5, sign * y, z + chord * thickness * 0.5],
            [x + chord, sign * y, z],
            [x + chord * 0.5, sign * y, z - chord * thickness * 0.5],
        ]);
    }
    let mut triangles = vec![[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6]];
    for i in 0..4 {
        let j = (i + 1) % 4;
        triangles.extend([[i, j, j + 4], [i, j + 4, i + 4]]);
    }
    SurfacePart {
        vertices,
        triangles,
    }
}

pub(super) fn area(part: &SurfacePart) -> PhysicsResult<f64> {
    let mut total = 0.0;
    for face in &part.triangles {
        let vertex = |i| {
            part.vertices
                .get(i)
                .copied()
                .ok_or_else(|| PhysicsError::invalid("mesh", "invalid vertex index"))
        };
        let a = vertex(face[0])?;
        let b = vertex(face[1])?;
        let c = vertex(face[2])?;
        let u: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let v: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        total += cross.iter().map(|x| x * x).sum::<f64>().sqrt() * 0.5;
    }
    Ok(total)
}
