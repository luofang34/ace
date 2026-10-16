use crate::mesh::Cell;

pub(super) fn neighbor_change(cell: &Cell, other: &Cell) -> [f64; 8] {
    let (Some(m), Some(n)) = (&cell.radial_moments, &other.radial_moments) else {
        return [0.0; 8];
    };
    let own = super::axisymmetry::moment_coordinates(cell)[0];
    let neighbor = super::axisymmetry::moment_coordinates(other)[0];
    let x = neighbor[0] - own[0];
    let z = (neighbor[1] - own[1]) / (2.0 * cell.center[1]);
    let ratio = other.center[1] / cell.center[1];
    let covariance = [
        n.scalar_covariance[0],
        ratio * n.scalar_covariance[1],
        ratio * ratio * n.scalar_covariance[2],
    ];
    [
        x,
        z,
        0.5 * (x * x + covariance[0] - m.scalar_covariance[0]),
        x * z + covariance[1] - m.scalar_covariance[1],
        0.5 * (z * z + covariance[2] - m.scalar_covariance[2]),
        (x.powi(3) + 3.0 * x * covariance[0] + n.scalar_third_x - m.scalar_third_x) / 6.0,
        0.5 * (x * x * z
            + z * covariance[0]
            + 2.0 * x * covariance[1]
            + ratio * n.scalar_mixed_xxz
            - m.scalar_mixed_xxz),
        (x.powi(4) + 6.0 * x * x * covariance[0] + 4.0 * x * n.scalar_third_x + n.scalar_fourth_x
            - m.scalar_fourth_x)
            / 24.0,
    ]
}
