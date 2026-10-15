use super::{MissionReport, evaluate_mission_with};
use crate::{PhysicsError, PhysicsResult};
use ace_engine::Registry;
use ace_model::{AnalysisResources, Binding, LoadingCase, MassKind, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bounded dispatch-fuel search using the same mission execution as the operating case.
#[derive(Debug, Clone, Copy)]
pub struct FuelSizing {
    /// Required fuel remaining at the end of every normal or contingency path.
    pub minimum_remaining_kg: f64,
    /// Maximum final bracket width in kilograms.
    pub tolerance_kg: f64,
    /// Maximum bisection iterations.
    pub max_iterations: u32,
}

/// A physically executable upper bound on required dispatch fuel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelSizingReport {
    /// Required loaded fuel, conservatively rounded to the feasible search boundary.
    pub required_fuel_kg: f64,
    /// Sum of declared usable tank capacities.
    pub usable_capacity_kg: f64,
    /// Fuel retained at the end of the most demanding scenario.
    pub minimum_remaining_kg: f64,
    /// Final feasible/infeasible bracket width.
    pub bracket_width_kg: f64,
    /// Completed bisection iterations.
    pub iterations: u32,
    /// Replayable mission at the required fuel load.
    pub verification: MissionReport,
}

/// Find a load satisfying all mission paths without exceeding any tank capacity.
/// Tank allocation is proportional to declared usable capacities.
pub fn size_mission_fuel(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
    registry: &Registry,
    settings: FuelSizing,
) -> PhysicsResult<FuelSizingReport> {
    validate(settings)?;
    registry.control().checkpoint()?;
    let capacities = capacities(resources, values)?;
    let capacity: f64 = capacities.values().sum();
    let trial = |fuel| evaluate_trial(id, resources, values, registry, &capacities, fuel);
    let mut upper = capacity;
    let mut lower = 0.0;
    let mut best = trial(upper)?;
    if !feasible(&best, settings)? {
        return Err(PhysicsError::invalid(
            id,
            format!(
                "usable fuel capacity {capacity} kg cannot satisfy mission and terminal reserve: {}",
                best.scenario_failure().unwrap_or_else(|| format!(
                    "minimum remaining {} kg, required {} kg",
                    best.minimum_remaining_fuel_kg(),
                    settings.minimum_remaining_kg
                ))
            ),
        ));
    }
    let mut iterations = 0_u32;
    while upper - lower > settings.tolerance_kg && iterations < settings.max_iterations {
        registry.control().checkpoint()?;
        let middle = (lower + upper) * 0.5;
        let report = trial(middle)?;
        if feasible(&report, settings)? {
            upper = middle;
            best = report;
        } else {
            lower = middle;
        }
        iterations = iterations.wrapping_add(1);
    }
    if upper - lower > settings.tolerance_kg {
        return Err(PhysicsError::invalid(
            id,
            format!(
                "fuel sizing did not converge within {} iterations; bracket {} kg",
                settings.max_iterations,
                upper - lower
            ),
        ));
    }
    Ok(FuelSizingReport {
        required_fuel_kg: upper,
        usable_capacity_kg: capacity,
        minimum_remaining_kg: best.minimum_remaining_fuel_kg(),
        bracket_width_kg: upper - lower,
        iterations,
        verification: best,
    })
}

fn validate(settings: FuelSizing) -> PhysicsResult<()> {
    if !settings.minimum_remaining_kg.is_finite()
        || settings.minimum_remaining_kg < 0.0
        || !settings.tolerance_kg.is_finite()
        || settings.tolerance_kg <= 0.0
        || settings.max_iterations == 0
        || settings.max_iterations > 128
    {
        return Err(PhysicsError::invalid(
            "fuel sizing",
            "invalid reserve, tolerance or iteration bound",
        ));
    }
    Ok(())
}

fn capacities(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<BTreeMap<String, f64>> {
    let mut capacities = BTreeMap::new();
    for (id, item) in &resources.mass_items {
        if item.kind != MassKind::Fuel {
            continue;
        }
        let capacity = item
            .capacity
            .as_ref()
            .ok_or_else(|| {
                PhysicsError::invalid(
                    id,
                    "fuel sizing requires an explicit usable capacity for every tank",
                )
            })?
            .resolve(values, Unit::Kilogram)?
            .value;
        if capacity <= 0.0 {
            return Err(PhysicsError::invalid(
                id,
                "usable capacity must be positive",
            ));
        }
        capacities.insert(id.clone(), capacity);
    }
    if capacities.is_empty() || !capacities.values().sum::<f64>().is_finite() {
        return Err(PhysicsError::invalid(
            "fuel sizing",
            "finite positive usable capacity is required",
        ));
    }
    Ok(capacities)
}

fn evaluate_trial(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
    registry: &Registry,
    capacities: &BTreeMap<String, f64>,
    fuel: f64,
) -> PhysicsResult<MissionReport> {
    let mut trial = resources.clone();
    let mission = trial
        .missions
        .get_mut(id)
        .ok_or_else(|| PhysicsError::invalid(id, "mission is not selected"))?;
    let mut loading = mission
        .loading
        .as_ref()
        .and_then(|name| trial.loading_cases.get(name))
        .cloned()
        .unwrap_or(LoadingCase {
            masses: BTreeMap::new(),
        });
    let capacity: f64 = capacities.values().sum();
    for (id, available) in capacities {
        loading.masses.insert(
            id.clone(),
            Binding::Constant(Quantity::new(
                fuel * (available / capacity),
                Unit::Kilogram,
            )?),
        );
    }
    let loading_id = format!("fuel-sizing:{id}");
    mission.loading = Some(loading_id.clone());
    trial.loading_cases.insert(loading_id, loading);
    evaluate_mission_with(id, &trial, values, registry)
}

fn feasible(report: &MissionReport, settings: FuelSizing) -> PhysicsResult<bool> {
    if !report.completed && !report.fuel_exhausted {
        return Err(PhysicsError::invalid(
            &report.mission,
            report
                .failure
                .as_deref()
                .unwrap_or("mission did not complete"),
        ));
    }
    for branch in report.branches.values() {
        if !branch.completed && !branch.fuel_exhausted {
            return Err(PhysicsError::invalid(
                &branch.mission,
                branch
                    .failure
                    .as_deref()
                    .unwrap_or("scenario did not complete"),
            ));
        }
    }
    Ok(report.complete() && report.minimum_remaining_fuel_kg() >= settings.minimum_remaining_kg)
}
