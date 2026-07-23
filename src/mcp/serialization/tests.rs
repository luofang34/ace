use serde_json::json;

use super::attach_units;

#[test]
fn ranges_default_to_nautical_miles() {
    let result = attach_units(json!({ "range_m": 1852.0 }));
    assert_eq!(result["range_m"]["unit"], "m");
    assert_eq!(result["range_m"]["display_unit"], "nmi");
    assert_eq!(result["range_m"]["display_value"], 1.0);
}
