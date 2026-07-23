use approx::assert_relative_eq;

use super::{Dimension, QuantityOutput, parse_quantity};

#[test]
fn aviation_units_normalize_to_si() {
    let speed = parse_quantity("122 kt", Dimension::Speed);
    assert!(speed.is_ok());
    if let Ok(value) = speed {
        assert_relative_eq!(value, 62.762_222, epsilon = 1.0e-6);
    }
}

#[test]
fn unitless_physical_values_are_rejected() {
    let result = parse_quantity("122", Dimension::Speed);
    assert!(result.is_err());
}

#[test]
fn range_output_defaults_to_nautical_miles() {
    let result = QuantityOutput::range(1852.0);
    assert_eq!(result.display_unit, "nmi");
    assert_relative_eq!(result.display_value, 1.0);
}
