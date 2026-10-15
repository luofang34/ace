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
    assert!(Isa1976::new(0.0).evaluate(86_000.0).is_ok());
    assert!(Isa1976::new(0.0).evaluate(86_001.0).is_err());
}

#[test]
fn upper_layers_match_the_1976_standard_tables() {
    // (geopotential m, temperature K, pressure Pa) from the published 1976 tables.
    for (geopotential, temperature, pressure) in [
        (11_000.0, 216.65, 22_632.06),
        (20_000.0, 216.65, 5_474.889),
        (32_000.0, 228.65, 868.0187),
        (47_000.0, 270.65, 110.9063),
        (51_000.0, 270.65, 66.93887),
        (71_000.0, 214.65, 3.956420),
    ] {
        let radius = 6_356_766.0;
        let geometric = radius * geopotential / (radius - geopotential);
        let state = Isa1976::new(0.0).evaluate(geometric).unwrap();
        assert_relative_eq!(state.temperature_k, temperature, max_relative = 1e-6);
        assert_relative_eq!(state.pressure_pa, pressure, max_relative = 2e-4);
    }
}
