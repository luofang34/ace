use super::*;
use crate::capability::tests::capability;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

fn fleet(capability: VehicleCapability, units: u32, replacements: u32) -> Fleet {
    Fleet {
        capability,
        initial_units: units,
        replacement_lead_time: q(72.0, Unit::Hour),
        max_replacements: replacements,
        operators_per_station: 1.0,
        operator_cost_per_hour: q(80.0, Unit::DollarPerHour),
    }
}

/// One station held 24 h/day, 90 km from base at 50 m/s: 1 h transit and
/// 9 h on station per 10 h sortie, with 2 h turnaround.
fn campaign(fleets: BTreeMap<String, Fleet>, no_fly: f64) -> Campaign {
    Campaign {
        schema_version: 1,
        name: "persistent watch".into(),
        demand: Demand {
            stations: 1,
            transit_distance: q(90_000.0, Unit::Meter),
            daily_coverage: q(24.0, Unit::Hour),
            days: 60,
        },
        environment: Environment {
            no_fly_probability: no_fly,
            comms_loss_probability: 0.0,
            comms_loss_effectiveness: 1.0,
        },
        fleets,
        daily_success_fraction: 0.9,
        required_success_rate: 0.8,
        replications: 20,
        seed: 3,
    }
}

fn fleets(entries: Vec<(&str, Fleet)>) -> BTreeMap<String, Fleet> {
    entries
        .into_iter()
        .map(|(id, fleet)| (id.to_owned(), fleet))
        .collect()
}

#[test]
fn a_reliable_fleet_with_enough_cycles_holds_the_station_at_exact_cost() {
    let vehicle = capability("uas", 50_000.0, 10.0, 0.0);
    let record = campaign(fleets(vec![("pair", fleet(vehicle, 2, 0))]), 0.0)
        .run()
        .unwrap();
    let outcome = &record.outcomes["pair"];
    assert_eq!(outcome.success_rate, 1.0);
    assert_eq!(outcome.status, ProbabilityStatus::Pass);
    assert!((outcome.delivered_fraction - 1.0).abs() < 1e-12);
    assert!((outcome.station_time_per_sortie_h - 9.0).abs() < 1e-9);
    assert_eq!(outcome.losses, 0.0);
    assert_eq!(outcome.cost.acquisition_usd, 100_000.0);
    // Three sorties a day: 24 h on station plus three 1 h transits.
    assert!((outcome.cost.flight_usd - 60.0 * 27.0 * 100.0).abs() < 1e-6);
    assert!((outcome.cost.operators_usd - 60.0 * 24.0 * 80.0).abs() < 1e-6);
    assert_eq!(record.preferred.as_deref(), Some("pair"));
}

#[test]
fn too_few_vehicle_cycles_fail_the_threshold_and_nothing_is_preferred() {
    let vehicle = capability("uas", 50_000.0, 10.0, 0.0);
    let record = campaign(fleets(vec![("single", fleet(vehicle, 1, 0))]), 0.0)
        .run()
        .unwrap();
    let outcome = &record.outcomes["single"];
    assert_eq!(outcome.success_rate, 0.0);
    assert!((outcome.delivered_fraction - 0.75).abs() < 1e-12);
    assert_eq!(outcome.status, ProbabilityStatus::Fail);
    assert_eq!(record.preferred, None);
}

#[test]
fn every_fleet_faces_the_same_weather_draws() {
    let vehicle = capability("uas", 50_000.0, 10.0, 0.0);
    let record = campaign(
        fleets(vec![
            ("a", fleet(vehicle.clone(), 3, 0)),
            ("b", fleet(vehicle, 3, 0)),
        ]),
        0.3,
    )
    .run()
    .unwrap();
    let (a, b) = (&record.outcomes["a"], &record.outcomes["b"]);
    assert_eq!(a.success_rate, b.success_rate);
    assert!((a.success_rate - 0.7).abs() < 0.05);
    assert!(a.lower < a.success_rate && a.success_rate < a.upper);
}

#[test]
fn attrition_is_replaced_at_a_cost_and_unreplaced_losses_empty_the_station() {
    let fragile = capability("uas", 50_000.0, 10.0, 0.05);
    let record = campaign(
        fleets(vec![
            ("replaced", fleet(fragile.clone(), 3, 100)),
            ("unreplaced", fleet(fragile, 3, 0)),
        ]),
        0.0,
    )
    .run()
    .unwrap();
    let (replaced, unreplaced) = (&record.outcomes["replaced"], &record.outcomes["unreplaced"]);
    assert!(replaced.losses > 1.0 && replaced.replacements == replaced.losses);
    assert!(replaced.cost.acquisition_usd > 150_000.0);
    assert!(unreplaced.empty_days > 0.0);
    assert!(replaced.success_rate > unreplaced.success_rate);
    assert_eq!(unreplaced.replacements, 0.0);
}

#[test]
fn the_preferred_fleet_is_the_most_cost_effective_among_those_that_pass() {
    let cheap = capability("cheap", 40_000.0, 10.0, 0.002);
    let premium = capability("premium", 20_000_000.0, 30.0, 0.0001);
    let record = campaign(
        fleets(vec![
            ("many_cheap", fleet(cheap, 4, 40)),
            ("few_premium", fleet(premium, 2, 1)),
        ]),
        0.05,
    )
    .run()
    .unwrap();
    let passing: Vec<_> = record
        .outcomes
        .iter()
        .filter(|(_, outcome)| outcome.status == ProbabilityStatus::Pass)
        .collect();
    assert_eq!(passing.len(), 2);
    let best = passing
        .iter()
        .max_by(|a, b| {
            a.1.station_hours_per_million_usd
                .total_cmp(&b.1.station_hours_per_million_usd)
        })
        .map(|(id, _)| (*id).clone());
    assert_eq!(record.preferred, best);
    record.verify_integrity().unwrap();
}

#[test]
fn campaigns_replay_identically_and_reject_invalid_definitions() {
    let vehicle = capability("uas", 50_000.0, 10.0, 0.01);
    let definition = campaign(fleets(vec![("pair", fleet(vehicle.clone(), 2, 5))]), 0.1);
    let first = definition.run().unwrap();
    assert_eq!(first.id, definition.run().unwrap().id);
    let mut tampered = first.clone();
    tampered.preferred = None;
    assert!(tampered.verify_integrity().is_err());
    let mut bad = definition.clone();
    bad.replications = 1;
    assert!(bad.run().is_err());
    let mut bad = definition.clone();
    bad.demand.daily_coverage = q(25.0, Unit::Hour);
    assert!(bad.run().is_err());
    let mut bad = definition;
    let mut forged = vehicle;
    forged.values.unit_cost.value = 1.0;
    bad.fleets.insert("forged".into(), fleet(forged, 1, 0));
    assert!(bad.run().is_err());
}
