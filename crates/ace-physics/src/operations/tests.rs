use super::*;
use serde_json::json;

pub(super) fn fixture() -> Operations {
    serde_json::from_value(
        json!({"seats":{"value":100,"unit":"1"},"distance":{"value":1000,"unit":"nmi"},
        "load_factor":{"value":0.5,"unit":"1"},"revenue_per_passenger":{"value":300,"unit":"USD"},
        "span":"span","required_runway":{"value":1500,"unit":"m"},
        "costs":[{"id":"fuel","category":"fuel","amount":{"value":9000,"unit":"USD"}}],
        "airports":[{"id":"A","maximum_span":{"value":36,"unit":"m"},
            "runway":{"value":1800,"unit":"m"},"movement_fee":{"value":1000,"unit":"USD"}}]}),
    )
    .unwrap()
}

#[test]
fn costs_yield_and_airport_discontinuity_share_one_result() {
    let operations = fixture();
    let mut values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    let report = evaluate_operations(&operations, &values).unwrap();
    assert_eq!(report.trip_cost_usd, 10000.0);
    assert_eq!(report.casm_usd_per_nmi, 0.1);
    assert_eq!(report.passenger_cost_usd_per_nmi, Some(0.2));
    assert_eq!(report.profit_usd, Some(5000.0));
    assert!(report.network_compatible);
    values.insert("span".into(), Quantity::new(37.0, Unit::Meter).unwrap());
    let report = evaluate_operations(&operations, &values).unwrap();
    assert!(!report.network_compatible);
    assert_eq!(report.airports[0].span_margin_m, -1.0);
}

#[test]
fn zero_load_factor_keeps_available_seat_cost_and_undefined_passenger_cost() {
    let mut operations = fixture();
    operations.load_factor = ace_model::Binding::Constant(Quantity::new(0.0, Unit::Ratio).unwrap());
    let values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    let report = evaluate_operations(&operations, &values).unwrap();
    assert_eq!(report.casm_usd_per_nmi, 0.1);
    assert!(report.passenger_cost_usd_per_nmi.is_none());
    assert_eq!(report.profit_usd, Some(-10000.0));
}
