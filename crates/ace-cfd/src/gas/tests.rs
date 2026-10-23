use super::*;

#[test]
fn energy_round_trip_and_invalid_states() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let q = Primitive {
        density: 0.04,
        velocity: [1200.0, -30.0, 4.0],
        pressure: 500.0,
    };
    let restored = gas.primitive(gas.conservative(q), 7).unwrap();
    assert!((restored.pressure / q.pressure - 1.0).abs() < 1.0e-13);
    assert_eq!(restored.velocity, q.velocity);
    assert!(gas.primitive([0.0; 5], 8).is_err());
    assert!(gas.primitive([1.0, 10.0, 0.0, 0.0, 1.0], 9).is_err());
    assert!(gas.primitive([f64::NAN; 5], 10).is_err());
}
