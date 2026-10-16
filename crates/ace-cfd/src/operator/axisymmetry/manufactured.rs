use crate::{Conserved, SphereGrid};

const GAUSS_EIGHT: [(f64, f64); 8] = [
    (-0.9602898564975363, 0.1012285362903763),
    (-0.7966664774136267, 0.2223810344533745),
    (-0.525532409916329, 0.3137066458778873),
    (-0.1834346424956498, 0.362683783378362),
    (0.1834346424956498, 0.362683783378362),
    (0.525532409916329, 0.3137066458778873),
    (0.7966664774136267, 0.2223810344533745),
    (0.9602898564975363, 0.1012285362903763),
];

pub(super) fn cell_mean(
    grid: SphereGrid,
    i: usize,
    j: usize,
    field: impl Fn(f64, f64) -> (Conserved, Conserved),
) -> (Conserved, Conserved) {
    let radii = [
        crate::vector::norm(grid.node(i, j)),
        crate::vector::norm(grid.node(i, j + 1)),
        crate::vector::norm(grid.node(i + 1, j)),
        crate::vector::norm(grid.node(i + 1, j + 1)),
    ];
    let angle = std::f64::consts::FRAC_PI_2 / grid.angular_cells as f64;
    let mut volume = 0.0;
    let mut state = [0.0; 5];
    let mut residual = [0.0; 5];
    for (t, wt) in GAUSS_EIGHT {
        let s = 0.5 * (1.0 + t);
        let theta = angle * (j as f64 + s);
        let a = (1.0 - s) * radii[0] + s * radii[1];
        let b = (1.0 - s) * radii[2] + s * radii[3];
        for (r, wr) in GAUSS_EIGHT {
            let radius = a + (b - a) * (1.0 + r) / 2.0;
            let weight = wr * wt * (b - a) * radius * radius * theta.sin();
            let (u, rhs) = field(-radius * theta.cos(), radius * theta.sin());
            volume += weight;
            for k in 0..5 {
                state[k] += u[k] * weight;
                residual[k] += rhs[k] * weight;
            }
        }
    }
    (state.map(|v| v / volume), residual.map(|v| v / volume))
}

#[cfg(test)]
mod pressure;
#[cfg(test)]
mod stress;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod thermal;

#[cfg(test)]
mod equilibrium;
#[cfg(test)]
mod polynomial;
#[cfg(test)]
mod viscous_wave;
