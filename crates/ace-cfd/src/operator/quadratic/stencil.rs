use super::*;

#[derive(Default)]
pub(in crate::operator) struct Stencil {
    pub neighbors: Vec<(usize, [f64; 5])>,
}

fn neighbors(mesh: &Mesh, i: usize) -> Vec<usize> {
    let adjacent = |id: usize| {
        mesh.cells[id].faces.iter().filter_map(move |face| {
            let f = &mesh.faces[*face];
            f.neighbor.map(|n| if f.owner == id { n } else { f.owner })
        })
    };
    let mut result: Vec<_> = adjacent(i).collect();
    for n in result.clone() {
        for other in adjacent(n) {
            if other != i && !result.contains(&other) {
                result.push(other);
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

fn transform(c: [f64; 5], basis: [f64; 3]) -> [f64; 5] {
    let [a, b, d] = basis;
    [
        a * c[0] + b * c[1],
        d * c[1],
        a * a * c[2] + 2.0 * a * b * c[3] + b * b * c[4],
        d * (a * c[3] + b * c[4]),
        d * d * c[4],
    ]
}

fn inverse(mut matrix: [[f64; 10]; 5], cell: usize) -> Result<[[f64; 5]; 5], CfdError> {
    for k in 0..5 {
        matrix[k][k + 5] = 1.0;
    }
    let scale = matrix
        .iter()
        .enumerate()
        .map(|(i, row)| row[i].abs())
        .fold(0.0_f64, f64::max);
    for col in 0..5 {
        let mut pivot = col;
        for row in col + 1..5 {
            if matrix[row][col].abs() > matrix[pivot][col].abs() {
                pivot = row;
            }
        }
        if !matrix[pivot][col].is_finite() || matrix[pivot][col].abs() < 1e-12 * scale {
            return Err(CfdError::Configuration(format!(
                "rank-deficient quadratic moment reconstruction in cell {cell}"
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
        std::array::from_fn(|j| matrix[i][j + 5])
    }))
}

fn stencil(mesh: &Mesh, i: usize) -> Result<Stencil, CfdError> {
    let cell = &mesh.cells[i];
    let Some(m) = &cell.radial_moments else {
        return Ok(Stencil::default());
    };
    let ids = neighbors(mesh, i);
    if ids.len() < 5 {
        return Ok(Stencil::default());
    }
    // Whiten cell moments so thin, skew cells do not mix incompatible length scales.
    let l00 = m.scalar_covariance[0].sqrt();
    let l10 = m.scalar_covariance[1] / l00;
    let l11 = (m.scalar_covariance[2] - l10 * l10).sqrt();
    let basis = [1.0 / l00, -l10 / (l00 * l11), 1.0 / l11];
    let own_cov = covariance(m.scalar_covariance, basis);
    let own = axisymmetry::moment_coordinates(cell)[0];
    let mut rows = Vec::new();
    let mut matrix = [[0.0; 10]; 5];
    for n in ids {
        let other = &mesh.cells[n];
        let Some(nm) = &other.radial_moments else {
            return Ok(Stencil::default());
        };
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
        ];
        let weight = 1.0 / (x * x + z * z);
        for a in 0..5 {
            for b in 0..5 {
                matrix[a][b] += weight * row[a] * row[b];
            }
        }
        rows.push((n, row, weight));
    }
    let inv = inverse(matrix, i)?;
    Ok(Stencil {
        neighbors: rows
            .into_iter()
            .map(|(n, row, weight)| {
                let c = std::array::from_fn(|k| {
                    inv[k].iter().zip(row).map(|(a, b)| a * b * weight).sum()
                });
                (n, transform(c, basis))
            })
            .collect(),
    })
}

pub(in crate::operator) fn stencils(mesh: &Mesh) -> Result<Vec<Stencil>, CfdError> {
    (0..mesh.cells.len()).map(|i| stencil(mesh, i)).collect()
}
