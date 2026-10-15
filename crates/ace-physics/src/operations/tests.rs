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
    assert_eq!(report.casm_usd_per_nmi, Some(0.1));
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
    operations.load_factor = Some(ace_model::Binding::Constant(
        Quantity::new(0.0, Unit::Ratio).unwrap(),
    ));
    let values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    let report = evaluate_operations(&operations, &values).unwrap();
    assert_eq!(report.casm_usd_per_nmi, Some(0.1));
    assert!(report.passenger_cost_usd_per_nmi.is_none());
    assert_eq!(report.profit_usd, Some(-10000.0));
}

#[test]
fn zero_seats_use_trip_and_hourly_cost_without_passenger_metrics() {
    let mut operations = fixture();
    operations.seats = Some(ace_model::Binding::Constant(
        Quantity::new(0.0, Unit::Ratio).unwrap(),
    ));
    operations.revenue_per_passenger = None;
    operations.block_duration = Some(ace_model::Binding::Constant(
        Quantity::new(7200.0, Unit::Second).unwrap(),
    ));
    let values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    let report = evaluate_operations(&operations, &values).unwrap();
    assert_eq!(report.trip_cost_usd, 10000.0);
    assert_eq!(report.block_duration_h, Some(2.0));
    assert_eq!(report.cost_per_block_hour_usd, Some(5000.0));
    assert_eq!(report.casm_usd_per_nmi, None);
    assert_eq!(report.passenger_cost_usd_per_nmi, None);
    assert_eq!(report.revenue_usd, None);
    assert_eq!(report.profit_usd, None);
    assert!(serde_json::to_value(&report).unwrap()["casm_usd_per_nmi"].is_null());
    operations.block_duration = None;
    let report = evaluate_operations(&operations, &values).unwrap();
    assert_eq!(report.trip_cost_usd, 10000.0);
    assert_eq!(report.cost_per_block_hour_usd, None);
    assert!(
        serde_json::to_value(&operations)
            .unwrap()
            .get("block_duration")
            .is_none()
    );
}

#[test]
fn hourly_accounting_rejects_invalid_denominators_and_passenger_yield_without_seats() {
    let values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    let mut operations = fixture();
    for seats in [-1.0, 0.5, f64::from(u32::MAX) + 1.0] {
        operations.seats = Some(ace_model::Binding::Constant(
            Quantity::new(seats, Unit::Ratio).unwrap(),
        ));
        assert!(evaluate_operations(&operations, &values).is_err());
    }
    operations.seats = Some(ace_model::Binding::Constant(
        Quantity::new(0.0, Unit::Ratio).unwrap(),
    ));
    assert!(
        evaluate_operations(&operations, &values)
            .unwrap_err()
            .to_string()
            .contains("passenger yield")
    );
    operations.revenue_per_passenger = None;
    for hours in [0.0, -1.0] {
        operations.block_duration = Some(ace_model::Binding::Constant(
            Quantity::new(hours, Unit::Hour).unwrap(),
        ));
        assert!(evaluate_operations(&operations, &values).is_err());
    }
}

#[test]
fn uas_operations_without_seats_report_hourly_and_payload_hour_costs() {
    let operations: Operations = serde_json::from_value(json!({
        "distance": {"value": 300, "unit": "nmi"},
        "block_duration": {"value": 10, "unit": "h"},
        "payload": {"value": 50, "unit": "kg"},
        "costs": [{"id": "fuel", "category": "fuel", "amount": {"value": 2000, "unit": "USD"}}]
    }))
    .unwrap();
    let report = evaluate_operations(&operations, &BTreeMap::new()).unwrap();
    assert_eq!(report.seats, 0);
    assert_eq!(report.cost_per_block_hour_usd, Some(200.0));
    assert_eq!(report.cost_per_payload_kg_block_hour_usd, Some(4.0));
    assert_eq!(report.casm_usd_per_nmi, None);

    let mut seated = fixture();
    seated.load_factor = None;
    let values = BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())]);
    assert!(evaluate_operations(&seated, &values).is_err());
}
