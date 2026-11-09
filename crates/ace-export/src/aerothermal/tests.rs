use std::collections::BTreeMap;

use ace_cfd::{FlowOperator, IdealGas, Mesh, Primitive, SolverConfig, SphereGrid, solve};
use syspack::{MemoryPackageSource, SyspackManifest, VerifiedSyspack};

use super::*;

const STREAM: CapsuleFreestream = CapsuleFreestream {
    density_kg_m3: 1.0e-3,
    speed_m_s: 7_000.0,
    pressure_pa: 10.0,
};
const CP_MAX: f64 = 1.84;
const K: f64 = 1.7415e-4;
const APOLLO: CapsuleGeometry = CapsuleGeometry {
    base_radius_m: 1.9558,
    nose_radius_m: 4.69,
    apex_x_body_m: 0.0,
    pitch_damping_coeff: -10.0,
};

/// A modified-Newtonian wall over a sphere of `radius_m`, from the stagnation
/// point to the equator, heated as the Sutton-Graves relation says.
fn newtonian_wall(radius_m: f64, cells: usize) -> Vec<WallSample> {
    let q = 0.5 * STREAM.density_kg_m3 * STREAM.speed_m_s * STREAM.speed_m_s;
    let step = PI / 2.0 / cells as f64;
    let stagnation_heat = K * (STREAM.density_kg_m3 / radius_m).sqrt() * STREAM.speed_m_s.powi(3);
    (0..cells)
        .map(|cell| {
            let theta = (cell as f64 + 0.5) * step;
            let (sin, cos) = theta.sin_cos();
            WallSample {
                theta,
                theta_squared_mean: theta * theta,
                position: [-radius_m * cos, radius_m * sin, 0.0],
                heat_flux: stagnation_heat * cos,
                pressure: STREAM.pressure_pa + q * CP_MAX * cos * cos,
                area: 2.0 * PI * radius_m * radius_m * sin * step,
            }
        })
        .collect()
}

fn converged(wall: Vec<WallSample>) -> Solution {
    Solution {
        converged: true,
        steps: 1_000,
        state: Vec::new(),
        wall,
        history: Vec::new(),
    }
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} vs {expected}"
    );
}

#[test]
fn the_axial_coefficient_matches_the_newtonian_integral_over_the_cap() {
    let params =
        capsule_params_from_wall(&newtonian_wall(APOLLO.nose_radius_m, 900), &STREAM, &APOLLO)
            .unwrap();
    let rim_sine = APOLLO.base_radius_m / APOLLO.nose_radius_m;
    let cos_squared = 1.0 - rim_sine * rim_sine;
    close(
        params.axial_drag_coeff,
        CP_MAX * (1.0 + cos_squared) / 2.0,
        0.01,
    );
    close(
        params.normal_force_slope,
        CP_MAX * rim_sine * rim_sine / 2.0,
        1e-3,
    );
}

#[test]
fn apollos_shape_lands_near_the_coefficients_it_ships_with() {
    let params =
        capsule_params_from_wall(&newtonian_wall(APOLLO.nose_radius_m, 300), &STREAM, &APOLLO)
            .unwrap();
    close(params.normal_force_slope, 0.17, 0.02);
    close(params.sutton_graves_k, K, K * 0.01);
    assert_eq!(params.reference_point_body_m, DVec3::new(-4.69, 0.0, 0.0));
}

#[test]
fn the_size_of_the_solved_sphere_does_not_change_the_coefficients() {
    let large = capsule_params_from_wall(&newtonian_wall(4.69, 300), &STREAM, &APOLLO).unwrap();
    let small = capsule_params_from_wall(&newtonian_wall(0.01, 300), &STREAM, &APOLLO).unwrap();
    close(large.axial_drag_coeff, small.axial_drag_coeff, 1e-9);
    close(large.sutton_graves_k, small.sutton_graves_k, K * 1e-9);
}

#[test]
fn the_capsule_model_reads_back_the_stagnation_heating_that_was_solved() {
    let wall = newtonian_wall(APOLLO.nose_radius_m, 300);
    let spec = capsule_params_from_wall(&wall, &STREAM, &APOLLO).unwrap();
    let solved =
        K * (STREAM.density_kg_m3 / APOLLO.nose_radius_m).sqrt() * STREAM.speed_m_s.powi(3);
    let modelled = spec.sutton_graves_k
        * (STREAM.density_kg_m3 / spec.nose_radius_m).sqrt()
        * STREAM.speed_m_s.powi(3);
    close(modelled, solved, solved * 0.01);
}

#[test]
fn the_derived_dynamics_load_through_the_package_validator() {
    let dynamics = capsule_dynamics_from_solution(
        &converged(newtonian_wall(APOLLO.nose_radius_m, 300)),
        &STREAM,
        &APOLLO,
    )
    .unwrap();
    let manifest = SyspackManifest {
        schema_version: syspack::SCHEMA_VERSION,
        id: "capsule".to_owned(),
        name: "Capsule".to_owned(),
        version: "1".to_owned(),
        kind: syspack::SystemKind::ReentryCapsule,
        assets: BTreeMap::new(),
        components: vec![syspack::ComponentRecord {
            key: "shell".to_owned(),
            name: "Shell".to_owned(),
            parent: None,
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            dry_mass_kg: 5_000.0,
            dry_inertia_body: [
                [6_000.0, 0.0, 0.0],
                [0.0, 6_000.0, 0.0],
                [0.0, 0.0, 6_000.0],
            ],
            initial_fluid_mass_kg: 0.0,
            initial_payload_mass_kg: 0.0,
            attached: true,
            thermal_capacity_j_k: 0.0,
            nominal_power_w: 0.0,
        }],
        dynamics,
        controls: syspack::ControlsSpec::default(),
        devices: Vec::new(),
        instrument_pack: None,
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let package = VerifiedSyspack::load(&MemoryPackageSource::new(bytes, BTreeMap::new())).unwrap();
    assert_eq!(package.capsule_aero().unwrap().params().nose_radius_m, 4.69);
}

/// A real solver run over a 1 cm sphere, stopped after `steps` iterations.
fn solved_sphere(steps: usize) -> Solution {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let mesh = Mesh::sphere(SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 8,
        angular_cells: 8,
        stretching: 3.0,
        shock_shape_mach: None,
    })
    .unwrap();
    let inflow = Primitive {
        density: 1.0e-3,
        velocity: [1_000.0, 0.0, 0.0],
        pressure: 300.0,
    };
    let operator = FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
    let controls = SolverConfig {
        cfl: 0.1,
        max_steps: steps,
        first_order_steps: steps - 1,
        check_interval: steps,
        residual_tolerance: 1.0e-6,
        heat_flux_tolerance: 1.0e-5,
    };
    solve(&operator, controls, None).unwrap()
}

#[test]
fn a_real_solution_that_has_not_converged_is_refused() {
    let solution = solved_sphere(5);
    assert!(!solution.wall.is_empty());
    assert!(matches!(
        capsule_dynamics_from_solution(&solution, &STREAM, &APOLLO),
        Err(AceExportError::SolutionNotConverged { steps: 5 })
    ));
}

#[test]
fn a_real_solvers_wall_reduces_to_drag_and_heating_with_the_right_signs() {
    let solution = solved_sphere(400);
    let stagnation = solution
        .wall
        .iter()
        .min_by(|a, b| a.theta.total_cmp(&b.theta))
        .unwrap();
    let [x, y, _] = stagnation.position;
    assert!(x < 0.0 && y >= 0.0, "the stagnation point faces the stream");
    assert!((x.hypot(y) - 0.01).abs() < 1e-3);
    let stream = CapsuleFreestream {
        density_kg_m3: 1.0e-3,
        speed_m_s: 1_000.0,
        pressure_pa: 300.0,
    };
    let params = capsule_params_from_wall(
        &solution.wall,
        &stream,
        &CapsuleGeometry {
            base_radius_m: 0.004,
            nose_radius_m: 0.01,
            ..APOLLO
        },
    )
    .unwrap();
    assert!(params.axial_drag_coeff > 0.0, "{params:?}");
    assert!(params.normal_force_slope > 0.0, "{params:?}");
    assert!(params.sutton_graves_k > 0.0, "{params:?}");
}

#[test]
fn a_stream_that_cannot_normalise_is_refused() {
    let wall = newtonian_wall(4.69, 30);
    for (stream, name) in [
        (
            CapsuleFreestream {
                speed_m_s: 0.0,
                ..STREAM
            },
            "speed_m_s",
        ),
        (
            CapsuleFreestream {
                density_kg_m3: f64::NAN,
                ..STREAM
            },
            "density_kg_m3",
        ),
        (
            CapsuleFreestream {
                pressure_pa: -1.0,
                ..STREAM
            },
            "pressure_pa",
        ),
    ] {
        assert!(matches!(
            capsule_params_from_wall(&wall, &stream, &APOLLO),
            Err(AceExportError::InvalidFreestream { field, .. }) if field == name
        ));
    }
}

#[test]
fn a_wall_that_stops_short_of_the_rim_is_refused() {
    assert!(matches!(
        capsule_params_from_wall(&[], &STREAM, &APOLLO),
        Err(AceExportError::InvalidWall {
            detail: "no samples"
        })
    ));
    let mut wall = newtonian_wall(4.69, 300);
    wall.retain(|sample| sample.theta < 0.2);
    assert!(matches!(
        capsule_params_from_wall(&wall, &STREAM, &APOLLO),
        Err(AceExportError::InvalidWall { .. })
    ));
}

#[test]
fn a_shape_the_capsule_model_refuses_never_becomes_coefficients() {
    let tighter_than_its_rim = CapsuleGeometry {
        nose_radius_m: 1.0,
        ..APOLLO
    };
    assert!(matches!(
        capsule_params_from_wall(&newtonian_wall(1.0, 300), &STREAM, &tighter_than_its_rim),
        Err(AceExportError::InvalidCapsule(_))
    ));
}
