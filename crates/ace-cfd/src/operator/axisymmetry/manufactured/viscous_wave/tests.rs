use crate::{Conserved, Convection, FlowOperator, IdealGas, Mesh, Primitive, SphereGrid};

const AMPLITUDE: f64 = 50.0;
const VISCOSITY: f64 = 1.8e-3;

fn field(gas: IdealGas, x: f64, y: f64, mixed: bool, k: f64) -> (Conserved, Conserved) {
    let a = AMPLITUDE;
    let (c, s) = ((k * x).cos(), (k * x).sin());
    let b = if mixed { 1e4 } else { 0.0 };
    let transverse = if mixed { a * (k * y).cos() } else { 0.0 };
    let q = Primitive {
        density: 0.01,
        pressure: 3000.0,
        velocity: [a * c + transverse, y * b * c, 0.0],
    };
    let radial_laplacian = if mixed {
        -a * k * k * (k * y).cos() - a * k * (k * y).sin() / y
    } else {
        0.0
    };
    (
        gas.conservative(q),
        [
            0.0,
            -4.0 / 3.0 * a * k * k * c + radial_laplacian - 2.0 / 3.0 * b * k * s,
            -y * b * k * k * c,
            0.0,
            0.0,
        ],
    )
}

fn operator(nr: usize, nt: usize) -> (FlowOperator, SphereGrid) {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.019047715851066883,
        radial_cells: nr,
        angular_cells: nt,
        stretching: 2.2753462770828685,
        shock_shape_mach: Some(3.0),
    };
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: VISCOSITY,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 0.01,
        pressure: 3000.0,
        velocity: [0.0; 3],
    };
    (
        FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
            .unwrap()
            .with_convection(Convection::Ausmdv),
        grid,
    )
}

fn measurements(nr: usize, nt: usize, mixed: bool, frequency: f64) -> ([f64; 2], [f64; 2], f64) {
    let (mut op, grid) = operator(nr, nt);
    let data: Vec<_> = (0..nr * nt)
        .map(|id| {
            super::super::cell_mean(grid, id % nr, id / nr, |x, y| {
                field(op.gas, x, y, mixed, frequency)
            })
        })
        .collect();
    let state: Vec<_> = data.iter().map(|v| v.0).collect();
    let mut ws = op.workspace();
    let mut magnitudes = vec![[0.0; 2]; nr];
    let residuals = [1.0, 0.5, 0.25].map(|fraction| {
        op.gas.viscosity = fraction * VISCOSITY;
        op.evaluate(&state, true, &mut ws).unwrap();
        assert_eq!(ws.mean_state, state);
        for (i, magnitude) in magnitudes
            .iter_mut()
            .enumerate()
            .take(2 * nr / 3)
            .skip(nr / 3)
        {
            let cell = &op.mesh.cells[i];
            for id in cell.faces {
                let face = &op.mesh.faces[id];
                for point in face.integration_points() {
                    let flux = op.face_flux(&face.at_point(point), true, &ws);
                    for k in 0..2 {
                        magnitude[k] += flux[k + 1].abs() * point.area / cell.volume;
                    }
                }
            }
            let stress = crate::operator::axial::hoop_stress(&op, &ws, i);
            let pressure = crate::operator::pressure::planar_mean(
                cell,
                ws.primitive[i].pressure,
                ws.pressure[i],
            );
            magnitude[1] += (pressure - stress).abs() * cell.planar_area / cell.volume;
        }
        ws.residual.clone()
    });
    let mut error = [0.0_f64; 2];
    let mut noise = [0.0_f64; 2];
    let mut linearity = 0.0_f64;
    for i in nr / 3..2 * nr / 3 {
        let radius = op.mesh.cells[i].center[1];
        let scales =
            [1.0, frequency * radius].map(|v| 0.5 * VISCOSITY * AMPLITUDE * frequency.powi(2) * v);
        for k in 0..2 {
            let difference = residuals[0][i][k + 1] - residuals[1][i][k + 1];
            let second = residuals[1][i][k + 1] - residuals[2][i][k + 1];
            error[k] =
                error[k].max((difference - 0.5 * VISCOSITY * data[i].1[k + 1]).abs() / scales[k]);
            noise[k] = noise[k].max(1e-13 * magnitudes[i][k] / scales[k]);
            linearity = linearity.max((difference - 2.0 * second).abs() / scales[k]);
        }
    }
    (error, noise, linearity)
}

#[test]
fn angular_refinement_does_not_create_an_axial_viscous_force_pole() {
    let values = [48, 96, 192, 384].map(|nt| measurements(48, nt, false, 1500.0));
    // For u=cos(k*x), v=0 the radial stress divergence is exactly zero.
    // An O(r) discretization error may persist under fixed radial spacing;
    // a 1/r error must not be hidden by simultaneous radial refinement.
    let bound = 2.0 * (values[0].0[1] + values[0].1[1]);
    assert!(
        values
            .iter()
            .all(|(e, noise, linearity)| { e[1] - noise[1] <= bound && *linearity < 1e-9 }),
        "directional viscous error, arithmetic bound, linearity: {values:?}"
    );
}

#[test]
fn resolved_viscous_waves_converge_at_the_axis() {
    for mixed in [false, true] {
        for aspect in [1, 4] {
            // Keep truncation above the independently bounded cancellation floor,
            // including the most anisotropic fine grid.
            let values = [48, 96, 192].map(|n| measurements(n, aspect * n, mixed, 3000.0));
            let e = values.map(|v| v.0);
            assert!(
                (0..2).all(|k| e[0][k] / e[1][k] > 3.0
                    && e[1][k] / e[2][k] > 3.0
                    && e[2][k] < 1e-3
                    && e[2][k] > values[2].1[k])
                    && values.iter().all(|v| v.2 < 1e-9),
                "mixed={mixed}, aspect={aspect}, viscous error/noise/linearity={values:?}"
            );
        }
    }
}
