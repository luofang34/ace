use crate::domain::diagnostic::AexError;

use super::SweepVariable;

const ASPECT_RATIO_PATH: &str = "aircraft.geometry.wing.aspect_ratio";

#[test]
fn aspect_ratio_linear_bounds_are_dimensionless() -> Result<(), Box<dyn std::error::Error>> {
    let bare = SweepVariable::linear(ASPECT_RATIO_PATH.to_owned(), "7", "8", 2, false)?;
    let explicit = SweepVariable::linear(ASPECT_RATIO_PATH.to_owned(), "7 1", "8 1", 2, false)?;

    assert_eq!(bare.values, ["7.000000000000", "8.000000000000"]);
    assert_eq!(explicit.values, bare.values);
    Ok(())
}

#[test]
fn physical_linear_bounds_still_require_units() {
    assert!(matches!(
        SweepVariable::linear(
            "aircraft.geometry.wing.area".to_owned(),
            "14",
            "18",
            2,
            false
        ),
        Err(AexError::Validation {
            code: "AMBIGUOUS_UNITLESS_VALUE",
            ..
        })
    ));
}
