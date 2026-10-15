use super::*;
use crate::mission::{evaluate_mission, tests::fixture};
use ace_model::Binding;

fn limits(lower_mass: f64) -> Vec<CgLimit> {
    [lower_mass, 150.0]
        .into_iter()
        .map(|mass| CgLimit {
            mass: Binding::Constant(Quantity::new(mass, Unit::Kilogram).unwrap()),
            minimum: Binding::Constant(Quantity::new(0.3, Unit::Meter).unwrap()),
            maximum: Binding::Constant(Quantity::new(0.8, Unit::Meter).unwrap()),
        })
        .collect()
}

#[test]
fn tank_depletion_and_payload_release_produce_the_limiting_actual_cg() {
    let (resources, values) = fixture();
    let mission = evaluate_mission("sortie", &resources, &values).unwrap();
    let report = evaluate_cg_envelope(&mission, &limits(100.0), &values).unwrap();
    let limiting = report.limiting.unwrap();
    assert_eq!(limiting.stage, "reserve");
    assert_eq!(limiting.mass_kg, 111.0);
    assert!((limiting.cg_x_m - 32.0 / 111.0).abs() < 1e-12);
    assert!((limiting.margin_m.unwrap() - (32.0 / 111.0 - 0.3)).abs() < 1e-12);
    assert!(report.failure.is_none());
}

#[test]
fn out_of_domain_mass_and_missing_mission_states_have_no_feasible_margin() {
    let (resources, mut values) = fixture();
    let mission = evaluate_mission("sortie", &resources, &values).unwrap();
    let report = evaluate_cg_envelope(&mission, &limits(120.0), &values).unwrap();
    assert!(report.limiting.is_none());
    assert!(report.failure.unwrap().contains("undefined"));
    values.insert(
        "phase.fuel".into(),
        Quantity::new(16.0, Unit::Kilogram).unwrap(),
    );
    let failed = evaluate_mission("sortie", &resources, &values).unwrap();
    let report = evaluate_cg_envelope(&failed, &limits(100.0), &values).unwrap();
    assert!(report.limiting.is_none());
    assert!(report.failure.unwrap().contains("shortfall"));
}

#[test]
fn independent_contingency_states_are_checked() {
    let (mut resources, values) = fixture();
    let mission = resources.missions.get_mut("sortie").unwrap();
    let reserve = mission.stages.pop().unwrap();
    mission.branches.insert(
        "alternate".into(),
        ace_model::MissionBranch {
            from_stage: Some("combat".into()),
            stages: vec![reserve],
        },
    );
    let mission = evaluate_mission("sortie", &resources, &values).unwrap();
    let report = evaluate_cg_envelope(&mission, &limits(100.0), &values).unwrap();
    assert_eq!(report.limiting.unwrap().scenario, "sortie/alternate");
}
