use approx::assert_relative_eq;

use super::Isa1976;

#[test]
fn sea_level_matches_isa_reference() {
    let result = Isa1976::new(0.0).evaluate(0.0);
    assert!(result.is_ok());
    if let Ok(state) = result {
        assert_relative_eq!(state.temperature_k, 288.15, epsilon = 0.01);
        assert_relative_eq!(state.pressure_pa, 101_325.0, epsilon = 1.0);
        assert_relative_eq!(state.density_kg_m3, 1.225, epsilon = 0.001);
    }
}

#[test]
fn model_covers_required_altitude_range() {
    assert!(Isa1976::new(0.0).evaluate(-2000.0).is_ok());
    assert!(Isa1976::new(0.0).evaluate(20_000.0).is_ok());
    assert!(Isa1976::new(0.0).evaluate(20_001.0).is_err());
}
