use super::tests::fixture;
use super::*;
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PhaseIncrement, PhaseOutput, PluginIdentity,
    PluginOutput, PluginSpec, Registry, SolverError,
};
use std::sync::Arc;

struct FractionPhase {
    spec: PluginSpec,
}

impl AnalysisPlugin for FractionPhase {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        _: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        Err(SolverError::Failed("state is required".into()))
    }
    fn evaluate_phase(
        &self,
        _: &BTreeMap<String, Quantity>,
        state: &VehicleState,
        _: &AnalysisContext<'_>,
    ) -> Result<PhaseOutput, SolverError> {
        let mut mass: f64 = state.masses.values().map(|q| q.value).sum();
        let fraction = state.conditions["fuel_fraction"].value;
        let mut increments = Vec::new();
        for _ in 0..2 {
            let burn = mass * fraction;
            increments.push(PhaseIncrement {
                duration: Quantity::new(30.0, Unit::Second)?,
                distance: Quantity::new(300.0, Unit::Meter)?,
                fuel_burn: Quantity::new(burn, Unit::Kilogram)?,
                cost: Quantity::new(burn * 2.0, Unit::Dollar)?,
                conditions: BTreeMap::new(),
                details: serde_json::json!({"mass_kg":mass}),
            });
            mass -= burn;
        }
        Ok(PhaseOutput {
            increments,
            approximate: true,
            artifacts: Vec::new(),
            details: serde_json::json!({"method":"fraction_of_actual_mass"}),
        })
    }
}

fn registry() -> Registry {
    let mut registry = Registry::default();
    registry
        .register(Arc::new(FractionPhase {
            spec: PluginSpec {
                identity: PluginIdentity {
                    id: "test.phase".into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: "test-state-phase".into(),
                },
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
            },
        }))
        .unwrap();
    registry
}

fn stateful() -> AnalysisResources {
    let (mut resources, _) = fixture();
    let analysis = resources.stage_analyses.get_mut("phase").unwrap();
    analysis.mode = ace_model::AnalysisMode::Phase;
    analysis.outputs.clear();
    let stages = &mut resources.missions.get_mut("sortie").unwrap().stages;
    for (stage, fraction) in stages.iter_mut().zip([0.05, 0.02]) {
        stage.inputs.insert(
            "fuel_fraction".into(),
            ace_model::Binding::Constant(Quantity::new(fraction, Unit::Ratio).unwrap()),
        );
    }
    resources
}

#[test]
fn actual_mass_payload_and_entry_controls_drive_the_next_phase() {
    let resources = stateful();
    let report =
        evaluate_mission_with("sortie", &resources, &BTreeMap::new(), &registry()).unwrap();
    assert!(report.completed);
    // Two 5% burns, payload release, then two 2% burns: ((140 * .95²) - 5) * .98².
    let expected = (140.0 * 0.95_f64.powi(2) - 5.0) * 0.98_f64.powi(2);
    assert!((report.final_state.mass.total_mass_kg - expected).abs() < 1e-10);
    assert!((report.remaining_fuel_kg - (expected - 105.0)).abs() < 1e-10);
    assert_eq!(report.stages[0].trajectory.len(), 3);
    assert_eq!(report.stages[1].trajectory.len(), 2);
    let phase = report.stages[1].phase.as_ref().unwrap();
    assert!(
        (phase
            .initial_state
            .masses
            .values()
            .map(|q| q.value)
            .sum::<f64>()
            - 121.35)
            .abs()
            < 1e-10
    );
    assert_eq!(phase.initial_state.conditions["fuel_fraction"].value, 0.02);
    assert_eq!(phase.plugin.code_revision, "test-state-phase");
    let cg = ((expected - 105.0) * 2.0 + 5.0 * 4.0) / expected;
    assert!((report.final_state.mass.cg_m[0] - cg).abs() < 1e-10);
    assert!(report.stages.iter().all(|stage| stage.approximate));
}

#[test]
fn missing_state_solver_is_not_replaced_by_precomputed_quantities() {
    let (_, values) = fixture();
    let report = evaluate_mission("sortie", &stateful(), &values).unwrap();
    assert!(!report.completed);
    assert!(report.stages.is_empty());
    assert_eq!(report.remaining_fuel_kg, 30.0);
    assert!(
        report
            .failure
            .unwrap()
            .contains("test.phase@1 is unavailable")
    );
}

fn sizing_resources() -> AnalysisResources {
    let mut resources = stateful();
    for (id, capacity) in [("front", 10.0), ("rear", 20.0)] {
        resources.mass_items.get_mut(id).unwrap().capacity = Some(ace_model::Binding::Constant(
            Quantity::new(capacity, Unit::Kilogram).unwrap(),
        ));
    }
    let mission = resources.missions.get_mut("sortie").unwrap();
    let reserve = mission.stages.pop().unwrap();
    mission.branches.insert(
        "reserve".into(),
        ace_model::MissionBranch {
            from_stage: Some("combat".into()),
            stages: vec![reserve],
        },
    );
    resources
}

#[test]
fn reserve_branch_keeps_normal_trip_state_and_cost_independent() {
    let report =
        evaluate_mission_with("sortie", &sizing_resources(), &BTreeMap::new(), &registry())
            .unwrap();
    assert!(report.complete());
    assert_eq!(report.stages.len(), 1);
    assert!((report.remaining_fuel_kg - 16.35).abs() < 1e-10);
    assert!((report.trip_cost_usd - 27.3).abs() < 1e-10);
    let reserve = &report.branches["reserve"];
    assert!((reserve.initial.mass.total_mass_kg - 121.35).abs() < 1e-10);
    assert!((reserve.remaining_fuel_kg - 11.54454).abs() < 1e-10);
    assert!((report.reserve_fuel_kg - 4.80546).abs() < 1e-10);
}

#[test]
fn fuel_demand_closes_to_the_independent_mass_balance_and_respects_capacity() {
    let mut resources = sizing_resources();
    let settings = FuelSizing {
        minimum_remaining_kg: 1.0,
        tolerance_kg: 1e-7,
        max_iterations: 64,
    };
    let result = size_mission_fuel(
        "sortie",
        &resources,
        &BTreeMap::new(),
        &registry(),
        settings,
    )
    .unwrap();
    let expected = ((106.0 / 0.98_f64.powi(2) + 5.0) / 0.95_f64.powi(2)) - 110.0;
    assert!((result.required_fuel_kg - expected).abs() < settings.tolerance_kg);
    assert!(result.minimum_remaining_kg >= 1.0);
    assert!(result.bracket_width_kg <= settings.tolerance_kg);
    assert_eq!(result.usable_capacity_kg, 30.0);
    assert!(result.verification.complete());
    for item in resources
        .mass_items
        .values_mut()
        .filter(|item| item.kind == MassKind::Fuel)
    {
        item.capacity = Some(ace_model::Binding::Constant(
            Quantity::new(1.0, Unit::Kilogram).unwrap(),
        ));
    }
    let error = size_mission_fuel(
        "sortie",
        &resources,
        &BTreeMap::new(),
        &registry(),
        settings,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("usable fuel capacity 2 kg cannot satisfy")
    );
}
