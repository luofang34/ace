use super::*;

#[test]
fn overflowing_geometry_and_cell_counts_are_rejected() {
    let grid = SphereGrid {
        radius: 1.0e200,
        outer_radius: 2.0e200,
        radial_cells: 4,
        angular_cells: 4,
        stretching: 2.0,
        shock_shape_mach: None,
    };
    assert!(Mesh::sphere(grid).is_err());
    assert!(
        Mesh::sphere(SphereGrid {
            radius: 1.0,
            outer_radius: 2.0,
            radial_cells: usize::MAX,
            ..grid
        })
        .is_err()
    );
}

#[test]
fn metric_identity_cancels_axisymmetric_uniform_pressure() {
    for guide in [None, Some(3.0)] {
        check_metric_identity(guide);
    }
}

fn check_metric_identity(guide: Option<f64>) {
    let mesh = Mesh::sphere(SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 16,
        angular_cells: 24,
        stretching: 3.0,
        shock_shape_mach: guide,
    })
    .unwrap();
    for (i, cell) in mesh.cells.iter().enumerate() {
        assert!(cell.volume > 0.0);
        let mut sum = [0.0; 3];
        for id in cell.faces {
            let face = &mesh.faces[id];
            let sign = if face.owner == i { 1.0 } else { -1.0 };
            sum = add(sum, scale(face.normal, sign * face.area));
        }
        assert!(sum[0].abs() < 2.0e-18);
        assert!((sum[1] - cell.planar_area).abs() < 2.0e-18);
    }
}

#[test]
fn bow_shock_guide_keeps_body_and_axis_boundaries_and_rejects_invalid_mach() {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.015,
        radial_cells: 16,
        angular_cells: 24,
        stretching: 1.35,
        shock_shape_mach: Some(3.0),
    };
    let vertices = grid.vertices().unwrap();
    for row in vertices.chunks_exact(grid.radial_cells + 1) {
        assert!((norm(row[0]) - grid.radius).abs() < 1e-15);
        assert!(norm(row[grid.radial_cells]) > grid.radius);
    }
    assert!((vertices[grid.radial_cells][0] + grid.outer_radius).abs() < 1e-15);
    for mach in [0.5, 1.0, f64::NAN, f64::INFINITY, 1.0e200] {
        assert!(grid_with_mach(grid, mach).vertices().is_err());
    }
}

fn grid_with_mach(grid: SphereGrid, mach: f64) -> SphereGrid {
    SphereGrid {
        shock_shape_mach: Some(mach),
        ..grid
    }
}

#[test]
fn explicit_mesh_rejects_bad_boundaries_and_preserves_a_resting_gas() {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 8,
        angular_cells: 8,
        stretching: 2.0,
        shock_shape_mach: None,
    };
    let mut vertices = grid.vertices().unwrap();
    assert!(Mesh::from_vertices(grid, &vertices[..vertices.len() - 1]).is_err());
    vertices[1][1] = 1e-5;
    assert!(Mesh::from_vertices(grid, &vertices).is_err());
    vertices[1][1] = 0.0;
    for j in 1..8 {
        for i in 1..8 {
            vertices[j * 9 + i][0] += 1e-5 * ((i * j) as f64).sin();
        }
    }
    let mesh = Mesh::from_vertices(grid, &vertices).unwrap();
    let gas = crate::IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = crate::Primitive {
        pressure: 1000.0,
        density: 1000.0 / (287.05 * 300.0),
        velocity: [0.0; 3],
    };
    let op = crate::FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
    assert!(
        op.residual(&op.initial_state(), true)
            .unwrap()
            .iter()
            .flatten()
            .all(|r| r.abs() < 1e-5)
    );
    vertices.swap(4 * 9 + 3, 4 * 9 + 4);
    assert!(Mesh::from_vertices(grid, &vertices).is_err());
}

#[test]
fn revolved_volume_converges_to_spherical_shell() {
    let volume = |n| {
        Mesh::sphere(SphereGrid {
            radius: 1.0,
            outer_radius: 2.0,
            radial_cells: n,
            angular_cells: n,
            stretching: 2.0,
            shock_shape_mach: None,
        })
        .unwrap()
        .cells
        .iter()
        .map(|c| c.volume)
        .sum::<f64>()
    };
    let exact = 7.0 / 3.0;
    let errors = [8, 16, 32].map(|n| (volume(n) - exact).abs());
    assert!(errors[0] / errors[1] > 3.9);
    assert!(errors[1] / errors[2] > 3.9);
}

#[test]
fn wall_area_matches_revolved_frusta_and_converges_to_the_hemisphere() {
    let errors = [12, 24, 48].map(|n| {
        let mesh = Mesh::sphere(SphereGrid {
            radius: 1.0,
            outer_radius: 2.0,
            radial_cells: 4,
            angular_cells: n,
            stretching: 2.0,
            shock_shape_mach: None,
        })
        .unwrap();
        let area: f64 = mesh
            .faces
            .iter()
            .filter(|f| f.neighbor.is_none() && matches!(f.boundary, Boundary::Wall))
            .map(|f| f.area)
            .sum();
        let exact_faceted = (std::f64::consts::PI / (4.0 * n as f64)).cos();
        assert!((area - exact_faceted).abs() < 1.0e-14);
        (area - 1.0).abs()
    });
    assert!(errors[0] / errors[1] > 3.99);
    assert!(errors[1] / errors[2] > 3.99);
}
