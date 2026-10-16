use crate::{Convection, FlowOperator, Mesh, SphereGrid};

fn polynomial_errors(op: &FlowOperator, grid: SphereGrid, powers: (i32, i32)) -> [f64; 4] {
    let n = grid.radial_cells;
    let (px, py) = powers;
    let field =
        |x: f64, y: f64| 3000.0 + 20.0 * (x / grid.radius).powi(px) * (y / grid.radius).powi(py);
    let means: Vec<_> = (0..n * grid.angular_cells)
        .map(|id| {
            super::super::cell_mean(grid, id % n, id / n, |x, y| {
                ([field(x, y), 0.0, 0.0, 0.0, 0.0], [0.0; 5])
            })
            .0[0]
        })
        .collect();
    let mut peaks = [0.0_f64; 4];
    for (id, cell) in op.mesh.cells.iter().enumerate() {
        let c =
            crate::operator::pressure::coefficients(&op.pressure_stencils[id], |k| means[k], id);
        let reconstructed =
            |x, y| means[id] + crate::operator::pressure::change(cell, c, [x, y, 0.0]);
        for face in cell.faces {
            for point in op.mesh.faces[face].integration_points() {
                let [x, y, _] = point.center;
                peaks[0] = peaks[0].max((reconstructed(x, y) / field(x, y) - 1.0).abs());
            }
        }
        let integrated = super::super::cell_mean(grid, id % n, id / n, |x, y| {
            (
                [reconstructed(x, y), field(x, y) / y, 0.0, 0.0, 0.0],
                [0.0; 5],
            )
        })
        .0;
        let planar = crate::operator::pressure::planar_mean(cell, means[id], c);
        let exact_planar = integrated[1] * cell.volume / cell.planar_area;
        peaks[1] = peaks[1].max((integrated[0] / means[id] - 1.0).abs());
        peaks[2] = peaks[2].max((planar / exact_planar - 1.0).abs());
        for (other, row) in &op.pressure_stencils[id].comparisons {
            let predicted = means[id] + c.iter().zip(row).map(|(a, b)| a * b).sum::<f64>();
            peaks[3] = peaks[3].max((predicted / means[*other] - 1.0).abs());
        }
    }
    peaks
}

#[test]
fn even_quartic_reconstruction_preserves_faces_and_both_measures() {
    let powers = [
        (1, 0),
        (0, 2),
        (2, 0),
        (1, 2),
        (0, 4),
        (3, 0),
        (2, 2),
        (4, 0),
    ];
    for aspect in [1, 4] {
        for n in [12, 24] {
            let (base, mut grid) =
                super::super::super::tests::strain_operator(n, Convection::Ausmdv);
            grid.angular_cells = aspect * n;
            grid.shock_shape_mach = Some(3.0);
            let op = FlowOperator::new(
                Mesh::curved_sphere(grid).unwrap(),
                base.gas,
                base.inflow,
                300.0,
            )
            .unwrap();
            for powers in powers {
                let p = polynomial_errors(&op, grid, powers);
                assert!(
                    p[0] < 1e-9 && p[1] < 1e-10 && p[2] < 1e-9 && p[3] < 1e-9,
                    "n={n}, aspect={aspect}, powers={powers:?}, face/volume/planar/neighbor errors={p:?}"
                );
            }
        }
    }
}
