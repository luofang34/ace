use crate::bindings::quantity;
use crate::{MassState, PhysicsError, PhysicsResult, mass_state_in_frames};
use ace_model::{AnalysisResources, MassKind, Mission, Quantity, Unit, VehicleState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod envelope;
mod phase;
mod sizing;
mod transition;

pub use envelope::{CgReport, CgStation, CgWitness, evaluate_cg_envelope};
pub use sizing::{FuelSizing, FuelSizingReport, size_mission_fuel};

/// State and mass properties at a mission boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionPoint {
    /// Time, distance, conditions, cost and individual loaded masses.
    pub state: VehicleState,
    /// Recomputed CG and inertia at this boundary.
    pub mass: MassState,
}

/// Completed phase and the resulting vehicle state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Mission stage identifier.
    pub id: String,
    /// Domain-defined phase kind.
    pub kind: String,
    /// Analysis providing the transition quantities.
    pub analysis: String,
    /// Whether consumption belongs to the reserve policy.
    pub reserve: bool,
    /// Fuel consumed by this phase, kilograms.
    pub fuel_burn_kg: f64,
    /// Payload released in this phase, kilograms by item.
    pub released_kg: BTreeMap<String, f64>,
    /// State at the end of this successful transition.
    pub end: MissionPoint,
    /// Physical state before applying phase controls and integration increments.
    pub start: MissionPoint,
    /// Recomputed mass, fuel and CG at every integration boundary.
    pub trajectory: Vec<MissionPoint>,
    /// Replayable invocation for a state-driven phase.
    pub phase: Option<ace_engine::PhaseRun>,
    /// Whether the invoked phase reports approximate physics.
    pub approximate: bool,
}

/// Executed mission, including evidence of any stage that could not be reached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionReport {
    /// Selected mission identifier.
    pub mission: String,
    /// Starting loading distribution and conditions.
    pub initial: MissionPoint,
    /// Successfully executed stages in order.
    pub stages: Vec<StageResult>,
    /// Last valid state; equals initial when the first stage fails.
    pub final_state: MissionPoint,
    /// Consumption in non-reserve phases, kilograms.
    pub block_fuel_kg: f64,
    /// Consumption in reserve phases, kilograms.
    pub reserve_fuel_kg: f64,
    /// Fuel remaining in all tanks at the last valid state, kilograms.
    pub remaining_fuel_kg: f64,
    /// Whether every phase was completed.
    pub completed: bool,
    /// First failed stage, if any.
    pub failed_stage: Option<String>,
    /// Contextual failure diagnostic.
    pub failure: Option<String>,
    /// Whether the first failure was an insufficient loaded-fuel state.
    pub fuel_exhausted: bool,
    /// Independent reserve or alternate results starting at their declared boundary.
    pub branches: BTreeMap<String, MissionReport>,
    /// Non-reserve trip expense; contingency consumption is kept separate.
    pub trip_cost_usd: f64,
}

impl MissionReport {
    /// Whether the normal path and every executed contingency path completed.
    pub fn complete(&self) -> bool {
        self.completed && self.branches.values().all(Self::complete)
    }

    /// Fuel remaining in the most demanding completed scenario.
    pub fn minimum_remaining_fuel_kg(&self) -> f64 {
        self.branches
            .values()
            .map(Self::minimum_remaining_fuel_kg)
            .fold(self.remaining_fuel_kg, f64::min)
    }

    /// First failure across the normal trip and declared contingencies.
    pub fn scenario_failure(&self) -> Option<String> {
        self.failure.clone().or_else(|| {
            self.branches.iter().find_map(|(id, branch)| {
                branch
                    .scenario_failure()
                    .map(|message| format!("scenario {id}: {message}"))
            })
        })
    }
}

/// Execute one mission using declared phase-analysis output ports.
///
/// Phase outputs must include `duration`, `distance`, `fuel_burn`, and `cost`.
/// Optional `state.*` outputs replace the named condition at the phase boundary.
pub fn evaluate_mission(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MissionReport> {
    evaluate_mission_with(id, resources, values, &ace_engine::Registry::default())
}

/// Execute a mission with application-registered state-dependent phase adapters.
pub fn evaluate_mission_with(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
    registry: &ace_engine::Registry,
) -> PhysicsResult<MissionReport> {
    let mission = resources
        .missions
        .get(id)
        .ok_or_else(|| PhysicsError::invalid(id, "mission is not selected"))?;
    if mission.stages.is_empty() {
        return Err(PhysicsError::invalid(id, "mission has no stages"));
    }
    let initial = initialize(mission, resources, values)?;
    let context = ace_engine::AnalysisContext {
        resources: resources.clone(),
        values,
        registry,
    };
    let mut report = execute_path(id, &mission.stages, initial, &context);
    for (branch_id, branch) in &mission.branches {
        let initial = match &branch.from_stage {
            Some(id) => report
                .stages
                .iter()
                .find(|stage| &stage.id == id)
                .map(|stage| stage.end.clone()),
            None => Some(report.initial.clone()),
        };
        if let Some(initial) = initial {
            report.branches.insert(
                branch_id.clone(),
                execute_path(
                    &format!("{id}/{branch_id}"),
                    &branch.stages,
                    initial,
                    &context,
                ),
            );
        } else if report.completed {
            return Err(PhysicsError::invalid(
                branch_id,
                "branch entry boundary is unavailable",
            ));
        }
    }
    if report.complete() {
        report.reserve_fuel_kg = report.reserve_fuel_kg.max(
            fuel(&report.initial.mass) - report.minimum_remaining_fuel_kg() - report.block_fuel_kg,
        );
    }
    Ok(report)
}

fn execute_path(
    id: &str,
    stages: &[ace_model::MissionStage],
    initial: MissionPoint,
    context: &ace_engine::AnalysisContext<'_>,
) -> MissionReport {
    let mut report = MissionReport {
        mission: id.into(),
        remaining_fuel_kg: fuel(&initial.mass),
        final_state: initial.clone(),
        initial,
        stages: Vec::new(),
        block_fuel_kg: 0.0,
        reserve_fuel_kg: 0.0,
        completed: true,
        failed_stage: None,
        failure: None,
        fuel_exhausted: false,
        branches: BTreeMap::new(),
        trip_cost_usd: 0.0,
    };
    for stage in stages {
        match transition::advance(stage, &report.final_state, context) {
            Ok(result) => {
                if result.reserve {
                    report.reserve_fuel_kg += result.fuel_burn_kg;
                } else {
                    report.block_fuel_kg += result.fuel_burn_kg;
                    report.trip_cost_usd +=
                        result.end.state.cost.value - result.start.state.cost.value;
                }
                report.remaining_fuel_kg = fuel(&result.end.mass);
                report.final_state = result.end.clone();
                report.stages.push(result);
            }
            Err(error) => {
                report.completed = false;
                report.failed_stage = Some(stage.id.clone());
                report.failure = Some(error.to_string());
                report.fuel_exhausted = matches!(error, PhysicsError::FuelShortfall { .. });
                break;
            }
        }
    }
    report
}

fn initialize(
    mission: &Mission,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MissionPoint> {
    let mut overrides = BTreeMap::new();
    if let Some(id) = &mission.loading {
        let loading = resources
            .loading_cases
            .get(id)
            .ok_or_else(|| PhysicsError::invalid(id, "initial loading case is not selected"))?;
        for (id, binding) in &loading.masses {
            overrides.insert(id.clone(), binding.resolve(values, Unit::Kilogram)?);
        }
    }
    let mass = mass_state_in_frames(resources, values, &overrides)?;
    let masses = mass
        .items
        .iter()
        .map(|(id, item)| Ok((id.clone(), Quantity::new(item.mass_kg, Unit::Kilogram)?)))
        .collect::<PhysicsResult<_>>()?;
    let conditions = mission
        .conditions
        .iter()
        .map(|(id, binding)| Ok((id.clone(), quantity(binding, values)?)))
        .collect::<PhysicsResult<_>>()?;
    Ok(MissionPoint {
        state: VehicleState {
            time: Quantity::new(0.0, Unit::Second)?,
            distance: Quantity::new(0.0, Unit::Meter)?,
            cost: Quantity::new(0.0, Unit::Dollar)?,
            masses,
            conditions,
        },
        mass,
    })
}

fn fuel(state: &MassState) -> f64 {
    state
        .items
        .values()
        .filter(|item| item.kind == MassKind::Fuel)
        .map(|item| item.mass_kg)
        .sum()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "mission/stateful/tests.rs"]
mod stateful_tests;
