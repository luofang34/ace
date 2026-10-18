use super::*;
use crate::services::synthesis::flight::{FlightPhase, tests};
use ace_engine::{AnalysisContext, AnalysisPlugin, Registry};
use ace_model::{Analysis, AnalysisMode, Quantity, VehicleState};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

fn inputs(propeller: bool, polar: bool) -> BTreeMap<String, Quantity> {
    let mut values = tests::inputs();
    if propeller {
        values.remove("tsfc");
        values.remove("available_thrust");
        for (name, value, unit) in [
            ("shaft_power", 1e6, Unit::Watt),
            ("propeller_efficiency", 0.8, Unit::Ratio),
            ("bsfc", 2e-7, Unit::KilogramPerJoule),
            ("power_lapse_exponent", 0.0, Unit::Ratio),
        ] {
            values.insert(name.into(), Quantity::new(value, unit).unwrap());
        }
    }
    if polar {
        values.remove("oswald");
        let k = 100.0 / (std::f64::consts::PI * 0.8 * 35.0_f64.powi(2));
        for name in crate::backends::openvsp::polar::PORTS {
            let value = if name == "cl_limit" { 1.5 } else { k };
            values.insert(name.into(), Quantity::new(value, Unit::Ratio).unwrap());
        }
    }
    values
}

fn state() -> VehicleState {
    let mut state = tests::state(3000.0);
    for (name, value, unit) in [
        ("altitude", 0.0, Unit::Meter),
        ("speed", 60.0, Unit::MeterPerSecond),
        ("duration", 120.0, Unit::Second),
        ("target_altitude", 0.0, Unit::Meter),
        ("target_speed", 60.0, Unit::MeterPerSecond),
        ("path_distance", 7200.0, Unit::Meter),
        ("route_distance", 8400.0, Unit::Meter),
        ("arrival_distance", 1200.0, Unit::Meter),
    ] {
        state
            .conditions
            .insert(name.into(), Quantity::new(value, unit).unwrap());
    }
    state
}

fn reference_burn(propeller: bool) -> f64 {
    // Closed-form mass ODE at constant sea-level conditions; no mission integrator is used.
    let density = 101325.0 / (287.05287 * 288.15);
    let q_area = 0.5 * density * 60.0_f64.powi(2) * 100.0;
    let k = 100.0 / (std::f64::consts::PI * 0.8 * 35.0_f64.powi(2));
    let specific = if propeller {
        2e-7 * 60.0 / 0.8
    } else {
        0.000016
    };
    let a = specific * q_area * 0.02;
    let b = specific * k * 9.80665_f64.powi(2) / q_area;
    let final_mass =
        (a / b).sqrt() * ((3000.0 * (b / a).sqrt()).atan() - (a * b).sqrt() * 120.0).tan();
    3000.0 - final_mass
}

fn execute(kind: PhaseKind, propeller: bool, polar: bool) -> ace_engine::PhaseRun {
    let mut plugin = FlightPhase::with_kind(kind);
    if propeller {
        plugin = plugin.propeller();
    }
    if polar {
        plugin = plugin.vspaero();
    }
    let values = inputs(propeller, polar);
    let analysis = Analysis {
        mode: AnalysisMode::Phase,
        plugin: plugin.spec().identity.id.clone(),
        version: "1".into(),
        inputs: values.keys().map(|id| (id.clone(), id.clone())).collect(),
        outputs: BTreeMap::new(),
        geometry: vec![],
        requirements: vec![],
        resources: Default::default(),
    };
    let mut registry = Registry::default();
    registry.register(Arc::new(plugin)).unwrap();
    let context = AnalysisContext {
        resources: Default::default(),
        values: &values,
        registry: &registry,
    };
    registry
        .evaluate_phase(&analysis, &state(), &context)
        .unwrap()
}

#[test]
fn all_twelve_phase_combinations_match_the_same_independent_level_path_reference() {
    let mut ids = BTreeSet::new();
    for kind in [
        PhaseKind::Level,
        PhaseKind::RouteCruise,
        PhaseKind::EnergyPath,
    ] {
        for propeller in [false, true] {
            for polar in [false, true] {
                let run = execute(kind, propeller, polar);
                let burn: f64 = run
                    .output
                    .increments
                    .iter()
                    .map(|step| step.fuel_burn.value)
                    .sum();
                let cost: f64 = run
                    .output
                    .increments
                    .iter()
                    .map(|step| step.cost.value)
                    .sum();
                let distance: f64 = run
                    .output
                    .increments
                    .iter()
                    .map(|step| step.distance.value)
                    .sum();
                assert!(
                    (burn - reference_burn(propeller)).abs() < 1e-6,
                    "{}",
                    run.plugin.id
                );
                assert!((cost - burn * 1.25).abs() < 1e-9);
                assert!((distance - 7200.0).abs() < 1e-8);
                assert_eq!(run.contract.as_ref().unwrap().mode, AnalysisMode::Phase);
                assert!(run.output.approximate);
                assert!(ids.insert(run.plugin.id));
            }
        }
    }
    assert_eq!(ids.len(), 12);
}
