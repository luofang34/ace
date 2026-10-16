use super::*;

#[derive(Default)]
pub(in crate::operator) struct Stencil {
    pub neighbors: Vec<(usize, [f64; 8])>,
    pub comparisons: Vec<(usize, [f64; 8])>,
}

fn neighbors(i: usize, dimensions: [usize; 2]) -> Vec<usize> {
    let [nr, nt] = dimensions;
    if nr < 5 || nt < 3 {
        return Vec::new();
    }
    let (radial, angular) = (i % nr, i / nr);
    let start = radial.saturating_sub(2).min(nr - 5);
    let middle = radial.saturating_sub(1).min(nr - 3);
    let rows = if angular == 0 {
        [angular, 1, 2]
    } else if angular + 1 == nt {
        [angular, angular - 1, angular - 2]
    } else {
        [angular, angular - 1, angular + 1]
    };
    // Five common radial positions interpolate arbitrary row data without an
    // axial least-squares defect that can leak into the shrinking angular span.
    (start..start + 5)
        .map(|r| rows[0] * nr + r)
        .chain((middle..middle + 3).map(|r| rows[1] * nr + r))
        .chain(std::iter::once(rows[2] * nr + radial))
        .filter(|other| *other != i)
        .collect()
}

fn least_squares_neighbors(mesh: &Mesh, i: usize) -> Vec<usize> {
    let adjacent = |id: usize| {
        mesh.cells[id].faces.iter().filter_map(move |face| {
            let f = &mesh.faces[*face];
            f.neighbor.map(|n| if f.owner == id { n } else { f.owner })
        })
    };
    let mut result: Vec<_> = adjacent(i).collect();
    // A one-sided quartic fit needs five axial sampling positions at a boundary.
    for _ring in 0..3 {
        for n in result.clone() {
            for other in adjacent(n) {
                if other != i && !result.contains(&other) {
                    result.push(other);
                }
            }
        }
    }
    result
}

fn covariance(c: [f64; 3], basis: [f64; 3]) -> [f64; 3] {
    let [xx, xy, yy] = c;
    let [a, b, d] = basis;
    [
        a * a * xx,
        a * (b * xx + d * xy),
        b * b * xx + 2.0 * b * d * xy + d * d * yy,
    ]
}

fn transform(c: [f64; 8], basis: [f64; 3]) -> [f64; 8] {
    let [a, b, d] = basis;
    [
        a * c[0] + b * c[1],
        d * c[1],
        a * a * c[2] + 2.0 * a * b * c[3] + b * b * c[4],
        d * (a * c[3] + b * c[4]),
        d * d * c[4],
        a * a * a * c[5] + 3.0 * a * a * b * c[6],
        a * a * d * c[6],
        a.powi(4) * c[7],
    ]
}

fn inverse(mut matrix: [[f64; 16]; 8], cell: usize) -> Result<[[f64; 8]; 8], CfdError> {
    for k in 0..8 {
        matrix[k][k + 8] = 1.0;
    }
    let scale = matrix
        .iter()
        .flat_map(|row| row[..8].iter())
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);
    for col in 0..8 {
        let mut pivot = col;
        for row in col + 1..8 {
            if matrix[row][col].abs() > matrix[pivot][col].abs() {
                pivot = row;
            }
        }
        if !matrix[pivot][col].is_finite() || matrix[pivot][col].abs() < 1e-12 * scale {
            return Err(CfdError::Configuration(format!(
                "rank-deficient pressure reconstruction in cell {cell}"
            )));
        }
        matrix.swap(col, pivot);
        let divisor = matrix[col][col];
        for value in matrix[col].iter_mut().skip(col) {
            *value /= divisor;
        }
        let pivot_row = matrix[col];
        for (row, values) in matrix.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = values[col];
            for (value, pivot) in values.iter_mut().zip(pivot_row).skip(col) {
                *value -= factor * pivot;
            }
        }
    }
    Ok(std::array::from_fn(|i| {
        std::array::from_fn(|j| matrix[i][j + 8])
    }))
}

fn fit_row(cell: &Cell, other: &Cell, basis: [f64; 3]) -> Option<([f64; 8], f64)> {
    let m = cell.radial_moments.as_ref()?;
    let nm = other.radial_moments.as_ref()?;
    let own = axisymmetry::moment_coordinates(cell)[0];
    let own_cov = covariance(m.scalar_covariance, basis);
    let coord = axisymmetry::moment_coordinates(other)[0];
    let dx = coord[0] - own[0];
    let dz = (coord[1] - own[1]) / (2.0 * cell.center[1]);
    let [x, z] = [basis[0] * dx, basis[1] * dx + basis[2] * dz];
    let radius = other.center[1] / cell.center[1];
    let c = nm.scalar_covariance;
    let c = covariance([c[0], radius * c[1], radius * radius * c[2]], basis);
    let row = [
        x,
        z,
        0.5 * (x * x + c[0] - own_cov[0]),
        x * z + c[1] - own_cov[1],
        0.5 * (z * z + c[2] - own_cov[2]),
        (x.powi(3) + 3.0 * x * c[0] + basis[0].powi(3) * (nm.scalar_third_x - m.scalar_third_x))
            / 6.0,
        0.5 * (x * x * z
            + z * c[0]
            + 2.0 * x * c[1]
            + basis[0].powi(2)
                * (basis[1] * (nm.scalar_third_x - m.scalar_third_x)
                    + basis[2] * (radius * nm.scalar_mixed_xxz - m.scalar_mixed_xxz))),
        (x.powi(4)
            + 6.0 * x * x * c[0]
            + 4.0 * x * basis[0].powi(3) * nm.scalar_third_x
            + basis[0].powi(4) * (nm.scalar_fourth_x - m.scalar_fourth_x))
            / 24.0,
    ];
    let weight = 1.0 / (x * x + z * z);
    Some((row, weight))
}

fn stencil(
    mesh: &Mesh,
    i: usize,
    ids: Vec<usize>,
    interpolatory: bool,
) -> Result<Stencil, CfdError> {
    let cell = &mesh.cells[i];
    let Some(m) = &cell.radial_moments else {
        return Ok(Stencil::default());
    };
    if ids.len() < 8 {
        return Ok(Stencil::default());
    }
    // Whiten thin, skew cells before fitting powers with different length scales.
    let l00 = m.scalar_covariance[0].sqrt();
    let l10 = m.scalar_covariance[1] / l00;
    let l11 = (m.scalar_covariance[2] - l10 * l10).sqrt();
    let basis = [1.0 / l00, -l10 / (l00 * l11), 1.0 / l11];
    let mut rows = Vec::new();
    let mut matrix = [[0.0; 16]; 8];
    for (index, n) in ids.into_iter().enumerate() {
        let other = &mesh.cells[n];
        let Some((row, weight)) = fit_row(cell, other, basis) else {
            return Ok(Stencil::default());
        };
        if interpolatory {
            matrix[index][..8].copy_from_slice(&row);
        } else {
            for a in 0..8 {
                for b in 0..8 {
                    matrix[a][b] += weight * row[a] * row[b];
                }
            }
        }
        rows.push((n, row, weight));
    }

    let inv = inverse(matrix, i)?;
    Ok(Stencil {
        comparisons: least_squares_neighbors(mesh, i)
            .into_iter()
            .map(|n| (n, super::moments::neighbor_change(cell, &mesh.cells[n])))
            .collect(),
        neighbors: rows
            .into_iter()
            .enumerate()
            .map(|(j, (n, row, weight))| {
                let c = std::array::from_fn(|k| {
                    if interpolatory {
                        inv[k][j]
                    } else {
                        inv[k].iter().zip(row).map(|(a, b)| a * b * weight).sum()
                    }
                });
                (n, transform(c, basis))
            })
            .collect(),
    })
}

pub(in crate::operator) fn stencils(mesh: &Mesh) -> Result<Vec<Stencil>, CfdError> {
    let nr = mesh
        .faces
        .iter()
        .filter(|f| f.neighbor.is_none() && matches!(f.boundary, Boundary::Axis))
        .count();
    if nr == 0 || !mesh.cells.len().is_multiple_of(nr) {
        return Err(CfdError::Configuration(
            "pressure interpolation requires polar grid topology".into(),
        ));
    }
    let dimensions = [nr, mesh.cells.len() / nr];
    (0..mesh.cells.len())
        .map(|i| oriented_stencil(mesh, i, dimensions))
        .collect()
}

fn amplification(mesh: &Mesh, i: usize, stencil: &Stencil) -> f64 {
    let cell = &mesh.cells[i];
    let magnitude = |planar: bool, position: [f64; 3]| {
        let mut own = 1.0;
        let mut total = 0.0;
        for (_, weight) in &stencil.neighbors {
            let value = if planar {
                planar_mean(cell, 0.0, *weight)
            } else {
                change(cell, *weight, position)
            };
            own -= value;
            total += value.abs();
        }
        own.abs() + total
    };
    let mut maximum = magnitude(true, cell.center);
    for face in cell.faces {
        for point in mesh.faces[face].integration_points() {
            maximum = maximum.max(magnitude(false, point.center));
        }
    }
    if let Some(m) = &cell.radial_moments {
        for point in m.reconstruction_points {
            maximum = maximum.max(magnitude(false, point));
        }
    }
    maximum
}

fn oriented_stencil(mesh: &Mesh, i: usize, dimensions: [usize; 2]) -> Result<Stencil, CfdError> {
    let [nr, nt] = dimensions;
    let transposed = (i / nr) + (i % nr) * nt;
    let across = neighbors(transposed, [nt, nr])
        .into_iter()
        .map(|n| (n % nt) * nr + n / nt)
        .collect();
    let mut best: Option<(f64, Stencil)> = None;
    for ids in [neighbors(i, dimensions), across] {
        if ids.len() != 8 {
            continue;
        }
        if let Ok(fit) = stencil(mesh, i, ids, true) {
            let score = amplification(mesh, i, &fit);
            if score.is_finite() && best.as_ref().is_none_or(|(value, _)| score < *value) {
                best = Some((score, fit));
            }
        }
    }
    // Choose the orientation that least amplifies input rounding at the faces
    // and source. Compact interpolation can be singular on an extreme aspect ratio.
    best.map_or_else(
        || stencil(mesh, i, least_squares_neighbors(mesh, i), false),
        |(_, fit)| Ok(fit),
    )
}
