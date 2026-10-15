use super::*;

#[test]
fn inconsistent_solver_geometry_is_rejected_before_publication() {
    let expected = BTreeMap::from([("TotalProjectedSpan".into(), 35.0)]);
    assert!(validate_readback("ACE_GEOMETRY_TotalProjectedSpan=26.796042", &expected).is_err());
    assert!(validate_readback("ACE_GEOMETRY_TotalProjectedSpan=NaN", &expected).is_err());
    assert!(validate_readback("ACE_GEOMETRY_TotalProjectedSpan=35.000000", &expected).is_ok());
}
