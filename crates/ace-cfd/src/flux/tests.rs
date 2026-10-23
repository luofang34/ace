use super::*;

#[test]
fn splitting_fluxes_preserve_uniform_flow_and_reverse_faces() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 1.0,
        viscosity: 0.01,
        conductivity: 0.01,
    };
    let a = Primitive {
        density: 1.0,
        velocity: [0.3, 0.4, -0.2],
        pressure: 1.0,
    };
    let b = Primitive {
        density: 0.3,
        velocity: [-0.2, 0.7, 0.1],
        pressure: 0.2,
    };
    let n = [0.6, 0.0, 0.8];
    type Flux = fn(IdealGas, Primitive, Primitive, Vector, f64) -> Conserved;
    let solvers: [Flux; 2] = [ausm, ausmdv];
    for solver in solvers {
        let forward = solver(gas, a, b, n, 0.3);
        let reverse = solver(gas, b, a, n.map(|v| -v), 0.3);
        let uniform = solver(gas, a, a, n, 0.3);
        let exact = physical(gas, a, n);
        for k in 0..5 {
            assert!((forward[k] + reverse[k]).abs() < 1.0e-12);
            assert!((uniform[k] - exact[k]).abs() < 1.0e-12);
        }
    }
}

#[test]
fn ausmdv_advects_a_moving_contact_exactly() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 1.0,
        viscosity: 0.01,
        conductivity: 0.01,
    };
    let a = Primitive {
        density: 1.0,
        velocity: [0.1, 0.2, -0.1],
        pressure: 1.0,
    };
    let b = Primitive { density: 10.0, ..a };
    let flux = ausmdv(gas, a, b, [1.0, 0.0, 0.0], 0.0);
    let exact = physical(gas, a, [1.0, 0.0, 0.0]);
    for k in 0..5 {
        assert!((flux[k] - exact[k]).abs() < 1.0e-12);
    }
}

#[test]
fn inviscid_flux_is_consistent_and_conservative_under_face_reversal() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let a = Primitive {
        density: 1.0,
        velocity: [500.0, 12.0, -4.0],
        pressure: 100000.0,
    };
    let b = Primitive {
        density: 2.0,
        velocity: [200.0, -5.0, 8.0],
        pressure: 300000.0,
    };
    let n = [0.6, 0.8, 0.0];
    let forward = hlle(gas, a, b, n);
    let reverse = hlle(gas, b, a, n.map(|x| -x));
    let same = hlle(gas, a, a, n);
    let exact = physical(gas, a, n);
    for k in 0..5 {
        assert!((forward[k] + reverse[k]).abs() < 1.0e-7);
        assert!((same[k] - exact[k]).abs() < 1.0e-7);
    }
}

#[test]
fn couette_stress_and_fourier_heat_have_correct_sign_and_work() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 0.02,
        conductivity: 0.1,
    };
    let g = [[0.0, 10.0, 0.0], [0.0; 3], [0.0; 3], [0.0, 100.0, 0.0]];
    let f = viscous(gas, [4.0, 0.0, 0.0], g, [0.0, 1.0, 0.0], 0.0);
    assert!((f[1] - 0.2).abs() < 1.0e-14);
    assert!((f[4] - 10.8).abs() < 1.0e-14);
}

#[test]
fn hllc_preserves_stationary_contacts_and_reverses_conservatively() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let a = Primitive {
        density: 1.0,
        velocity: [0.0; 3],
        pressure: 100000.0,
    };
    let b = Primitive { density: 2.0, ..a };
    let flux = hllc(gas, a, b, [1.0, 0.0, 0.0]);
    assert_eq!(flux, [0.0, 100000.0, 0.0, 0.0, 0.0]);
    let c = Primitive {
        velocity: [600.0, 14.0, 12.0],
        pressure: 200000.0,
        ..a
    };
    let n = [0.6, 0.0, 0.8];
    let forward = hllc(gas, c, b, n);
    let reverse = hllc(gas, b, c, n.map(|x| -x));
    for k in 0..5 {
        assert!((forward[k] + reverse[k]).abs() < 1.0e-7);
    }
}

#[test]
fn stationary_normal_shock_preserves_rankine_hugoniot_flux() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 1.0,
        viscosity: 0.01,
        conductivity: 0.01,
    };
    let mach_squared = 9.0;
    let ratio = (gas.gamma + 1.0) * mach_squared / ((gas.gamma - 1.0) * mach_squared + 2.0);
    let left = Primitive {
        density: 1.0,
        velocity: [(gas.gamma * mach_squared).sqrt(), 0.0, 0.0],
        pressure: 1.0,
    };
    let right = Primitive {
        density: ratio,
        velocity: [left.velocity[0] / ratio, 0.0, 0.0],
        pressure: 1.0 + 2.0 * gas.gamma / (gas.gamma + 1.0) * (mach_squared - 1.0),
    };
    let exact = physical(gas, left, [1.0, 0.0, 0.0]);
    for flux in [
        hlle(gas, left, right, [1.0, 0.0, 0.0]),
        hllc(gas, left, right, [1.0, 0.0, 0.0]),
    ] {
        for k in 0..5 {
            assert!((flux[k] - exact[k]).abs() < 1.0e-12);
        }
    }
}

#[test]
fn splitting_fluxes_preserve_common_total_enthalpy() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let enthalpy = 9e5;
    let state = |density, velocity: Vector| Primitive {
        density,
        velocity,
        pressure: density * (gas.gamma - 1.0) / gas.gamma
            * (enthalpy - 0.5 * dot(velocity, velocity)),
    };
    // The inviscid energy flux must equal H times the mass flux whenever
    // both states share H, including shock blending and transonic splitting.
    for (left, right) in [
        (
            state(0.01, [1000.0, 0.0, 0.0]),
            state(0.03, [300.0, 200.0, 0.0]),
        ),
        (
            state(0.01, [100.0, 80.0, 0.0]),
            state(0.02, [-80.0, 140.0, 0.0]),
        ),
        (
            state(0.01, [-1000.0, 0.0, 0.0]),
            state(0.02, [1000.0, 0.0, 0.0]),
        ),
    ] {
        for normal in [[1.0, 0.0, 0.0], [0.6, 0.8, 0.0]] {
            for parameter in [0.0, 0.1, 0.5, 1.0] {
                for flux in [
                    ausm(gas, left, right, normal, parameter),
                    ausmdv(gas, left, right, normal, parameter),
                ] {
                    let expected = enthalpy * flux[0];
                    assert!(
                        (flux[4] - expected).abs() < 1e-12 * (flux[4].abs() + expected.abs() + 1.0),
                        "energy flux={}, H times mass flux={expected}",
                        flux[4]
                    );
                }
            }
        }
    }
}
