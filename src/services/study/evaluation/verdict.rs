use std::collections::BTreeMap;

use crate::domain::evidence::{ConstraintStatus, EvidenceConstraint};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::RequirementEvaluation;
use crate::domain::schema::Requirement;
use crate::services::requirements::hard_requirements_passed;

pub(super) fn candidate_is_feasible(
    declared: &[Requirement],
    evaluations: &[RequirementEvaluation],
    mission_completed: bool,
    native_feasible: bool,
    constraints: &[EvidenceConstraint],
    objectives_available: bool,
) -> bool {
    let hard_constraints_pass = constraints.iter().all(|constraint| {
        constraint.severity != "hard" || constraint.status == ConstraintStatus::Pass
    });
    hard_requirements_passed(mission_completed, declared, evaluations)
        && native_feasible
        && hard_constraints_pass
        && objectives_available
}

pub(super) fn insert_metric_aliases(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    declared: &[Requirement],
    evaluations: &[RequirementEvaluation],
) -> bool {
    if let Some(fuel) = metrics.get("mission.fuel_burn").cloned() {
        metrics.insert("mission.total_fuel".to_owned(), fuel);
    }
    if let Some(distance) = metrics.get("mission.simulated_range").cloned() {
        metrics.insert("mission.completed_distance".to_owned(), distance);
    }
    let mission_completed = metrics
        .get("mission.completed")
        .is_some_and(|metric| metric.value == 1.0);
    let hard_passed = hard_requirements_passed(mission_completed, declared, evaluations);
    metrics.insert(
        "feasibility.hard_constraints_passed".to_owned(),
        QuantityOutput::si(f64::from(u8::from(hard_passed)), "bool"),
    );
    mission_completed
}
