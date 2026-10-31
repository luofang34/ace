use super::parsing::positive_marker_number;

#[test]
fn rejects_nonpositive_or_nonfinite_compgeom_area() {
    for value in ["0", "-1", "NaN", "inf"] {
        let output = format!("ACE_WETTED_AREA_M2={value}");
        assert!(positive_marker_number(&output, "ACE_WETTED_AREA_M2=").is_err());
    }
}
