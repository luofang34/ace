use super::*;
use serde_json::json;

fn fixture() -> Operations {
    let mut operations = super::super::tests::fixture();
    operations.costs.clear();
    operations.basis = Some(ace_model::EconomicBasis {
        currency: "USD".into(),
        price_year: 2025,
        cost_scope: "declared operating and ownership costs".into(),
    });
    operations.economics = Some(serde_json::from_value(json!({"selected":"design",
        "rates":{"fuel":{"value":1,"unit":"USD/kg"},"crew_flight_hour":{"value":100,"unit":"USD/h"},
            "maintenance_flight_hour":{"value":50,"unit":"USD/h"},"maintenance_cycle":{"value":80,"unit":"USD"},
            "turnaround_hour":{"value":20,"unit":"USD/h"},"annual_ownership":{"value":800000,"unit":"USD"}},
        "utilization":{"flight_time":{"value":2000,"unit":"h"},"operating_time":{"value":2400,"unit":"h"}},
        "scenarios":{"design":{"distance":{"value":1000,"unit":"nmi"},"block_duration":{"value":2,"unit":"h"},
            "block_fuel":{"value":1000,"unit":"kg"},"turnaround":{"value":1,"unit":"h"}}}})).unwrap());
    operations
}

fn inputs() -> BTreeMap<String, Quantity> {
    BTreeMap::from([("span".into(), Quantity::new(35.0, Unit::Meter).unwrap())])
}

#[test]
fn flight_cycles_turnaround_and_utilization_match_independent_cost_accounting() {
    let mut operations = fixture();
    let baseline = crate::evaluate_operations(&operations, &inputs()).unwrap();
    assert_eq!(baseline.annual_flights, Some(800.0));
    assert_eq!(baseline.trip_cost_usd, 3400.0);
    assert_eq!(baseline.costs_usd["rate.ownership"], 1000.0);
    assert_eq!(baseline.casm_usd_per_nmi, Some(0.034));
    operations.load_factor = Some(Binding::Constant(Quantity::new(0.0, Unit::Ratio).unwrap()));
    let empty = crate::evaluate_operations(&operations, &inputs()).unwrap();
    assert_eq!(empty.casm_usd_per_nmi, baseline.casm_usd_per_nmi);
    assert_eq!(empty.passenger_cost_usd_per_nmi, None);
    operations
        .economics
        .as_mut()
        .unwrap()
        .utilization
        .flight_time = Binding::Constant(Quantity::new(1000.0, Unit::Hour).unwrap());
    let reduced = crate::evaluate_operations(&operations, &inputs()).unwrap();
    assert_eq!(reduced.annual_flights, Some(500.0));
    assert_eq!(reduced.trip_cost_usd, 4000.0);
}

#[test]
fn all_route_airports_remain_hard_gates_and_monetary_bases_cannot_be_mixed() {
    let mut operations = fixture();
    let mut alternate = operations.economics.as_ref().unwrap().scenarios["design"].clone();
    let mut airport = operations.airports[0].clone();
    airport.maximum_span = Binding::Constant(Quantity::new(34.0, Unit::Meter).unwrap());
    alternate.airports = Some(vec![airport]);
    alternate.turnaround = Binding::Constant(Quantity::new(2.0, Unit::Hour).unwrap());
    operations
        .economics
        .as_mut()
        .unwrap()
        .scenarios
        .insert("alternate".into(), alternate);
    let result = crate::evaluate_operations(&operations, &inputs()).unwrap();
    assert!(result.airports[0].compatible);
    assert!(!result.network_compatible);
    assert_eq!(result.scenarios["alternate"].annual_flights, Some(600.0));
    assert!(
        (result.scenarios["alternate"].costs_usd["rate.ownership"] - 800_000.0 / 600.0).abs()
            < 1e-9
    );
    let mut imported_basis = operations.basis.clone().unwrap();
    imported_basis.price_year = 2024;
    operations.costs.push(CostItem {
        id: "navigation".into(),
        category: "navigation".into(),
        amount: Binding::Constant(Quantity::new(10.0, Unit::Dollar).unwrap()),
        basis: Some(imported_basis),
    });
    assert!(crate::evaluate_operations(&operations, &inputs()).is_err());
    operations.costs.clear();
    operations.basis.as_mut().unwrap().currency = "EUR".into();
    assert!(crate::evaluate_operations(&operations, &inputs()).is_err());
}

#[test]
fn zero_seat_scenarios_use_their_own_duration_and_preserve_all_airport_gates() {
    let mut operations = fixture();
    operations.seats = Some(Binding::Constant(Quantity::new(0.0, Unit::Ratio).unwrap()));
    operations.revenue_per_passenger = None;
    operations.block_duration = Some(Binding::Constant(Quantity::new(99.0, Unit::Hour).unwrap()));
    let mut alternate = operations.economics.as_ref().unwrap().scenarios["design"].clone();
    alternate.block_duration = Binding::Constant(Quantity::new(4.0, Unit::Hour).unwrap());
    let mut airport = operations.airports[0].clone();
    airport.maximum_span = Binding::Constant(Quantity::new(34.0, Unit::Meter).unwrap());
    alternate.airports = Some(vec![airport]);
    operations
        .economics
        .as_mut()
        .unwrap()
        .scenarios
        .insert("alternate".into(), alternate);
    let report = crate::evaluate_operations(&operations, &inputs()).unwrap();
    assert_eq!(report.trip_cost_usd, 3400.0);
    assert_eq!(report.cost_per_block_hour_usd, Some(1700.0));
    assert_eq!(report.block_duration_h, Some(2.0));
    let alternate = &report.scenarios["alternate"];
    // 480 cycles/year; fuel 1000 + hourly 600 + cycle 80 + ground 20 + fees 1000.
    let expected = (2700.0 + 800_000.0 / 480.0) / 4.0;
    assert!((alternate.cost_per_block_hour_usd.unwrap() - expected).abs() < 1e-9);
    assert_eq!(alternate.block_duration_h, Some(4.0));
    assert_eq!(alternate.casm_usd_per_nmi, None);
    assert!(report.airports[0].compatible);
    assert!(!report.network_compatible);
}
