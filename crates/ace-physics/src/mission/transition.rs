use super::{MissionPoint, StageResult};
use crate::bindings::{nonnegative, quantity, scalar};
use crate::{PhysicsError, PhysicsResult, mass_state_in_frames};
use ace_engine::{AnalysisContext, PhaseIncrement};
use ace_model::{AnalysisResources, MassKind, MissionStage, Quantity, Unit};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn advance(
    stage: &MissionStage,
    previous: &MissionPoint,
    context: &AnalysisContext<'_>,
) -> PhysicsResult<StageResult> {
    context.registry.control().checkpoint()?;
    let mut point = previous.clone();
    for (id, binding) in &stage.inputs {
        let value = quantity(binding, context.values)?;
        if let Some(before) = point.state.conditions.get(id) {
            value.to(before.unit)?;
        }
        point.state.conditions.insert(id.clone(), value);
    }
    let (increments, phase) = super::phase::solve(stage, &point.state, context)?;
    let approximate = phase.as_ref().is_some_and(|run| run.output.approximate);
    let mut trajectory = Vec::new();
    let mut fuel_burn_kg = 0.0;
    for increment in &increments {
        context.registry.control().checkpoint()?;
        point = integrate(stage, &point, increment, context)?;
        fuel_burn_kg += increment.fuel_burn.to(Unit::Kilogram)?.value;
        trajectory.push(point.clone());
    }
    let released_kg = release(
        stage,
        &mut point.state.masses,
        &context.resources,
        context.values,
    )?;
    point.mass = mass_state_in_frames(&context.resources, context.values, &point.state.masses)?;
    if !released_kg.is_empty() {
        trajectory.push(point.clone());
    }
    Ok(StageResult {
        id: stage.id.clone(),
        kind: stage.kind.clone(),
        analysis: stage.analysis.clone(),
        reserve: stage.reserve,
        fuel_burn_kg,
        released_kg,
        end: point,
        start: previous.clone(),
        trajectory,
        phase,
        approximate,
    })
}

fn integrate(
    stage: &MissionStage,
    previous: &MissionPoint,
    increment: &PhaseIncrement,
    context: &AnalysisContext<'_>,
) -> PhysicsResult<MissionPoint> {
    let duration = nonnegative(&stage.id, increment.duration.to(Unit::Second)?.value)?;
    let distance = nonnegative(&stage.id, increment.distance.to(Unit::Meter)?.value)?;
    let burn = nonnegative(&stage.id, increment.fuel_burn.to(Unit::Kilogram)?.value)?;
    let cost = nonnegative(&stage.id, increment.cost.to(Unit::Dollar)?.value)?;
    let mut state = previous.state.clone();
    consume(stage, burn, &mut state.masses, &context.resources)?;
    for (id, value) in &increment.conditions {
        if let Some(before) = state.conditions.get(id) {
            value.to(before.unit)?;
        }
        state.conditions.insert(id.clone(), *value);
    }
    state.time = Quantity::new(state.time.to(Unit::Second)?.value + duration, Unit::Second)?;
    state.distance = Quantity::new(
        state.distance.to(Unit::Meter)?.value + distance,
        Unit::Meter,
    )?;
    state.cost = Quantity::new(state.cost.to(Unit::Dollar)?.value + cost, Unit::Dollar)?;
    let mass = mass_state_in_frames(&context.resources, context.values, &state.masses)?;
    Ok(MissionPoint { state, mass })
}

fn consume(
    stage: &MissionStage,
    burn: f64,
    masses: &mut BTreeMap<String, Quantity>,
    resources: &AnalysisResources,
) -> PhysicsResult<()> {
    let mut remaining = burn;
    let mut seen = BTreeSet::new();
    for id in &stage.fuel_sequence {
        if !seen.insert(id)
            || resources
                .mass_items
                .get(id)
                .is_none_or(|item| item.kind != MassKind::Fuel)
        {
            return Err(PhysicsError::invalid(
                &stage.id,
                format!("invalid or duplicate tank {id}"),
            ));
        }
        let mass = masses
            .get_mut(id)
            .ok_or_else(|| PhysicsError::invalid(&stage.id, format!("tank {id} is not loaded")))?;
        let available = mass.to(Unit::Kilogram)?.value;
        let used = remaining.min(available);
        remaining -= used;
        *mass = Quantity::new(available - used, Unit::Kilogram)?;
    }
    // Depleting several tanks can leave an ulp-scale subtraction residual.
    if remaining > 16.0 * f64::EPSILON * burn.max(1.0) {
        return Err(PhysicsError::FuelShortfall {
            stage: stage.id.clone(),
            missing_kg: remaining,
        });
    }
    Ok(())
}

fn release(
    stage: &MissionStage,
    masses: &mut BTreeMap<String, Quantity>,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<BTreeMap<String, f64>> {
    let mut released = BTreeMap::new();
    for (id, binding) in &stage.payload_release {
        if resources
            .mass_items
            .get(id)
            .is_none_or(|item| item.kind != MassKind::Payload)
        {
            return Err(PhysicsError::invalid(
                &stage.id,
                format!("{id} is not a payload item"),
            ));
        }
        let release = nonnegative(id, scalar(binding, values, Unit::Kilogram)?)?;
        let mass = masses
            .get_mut(id)
            .ok_or_else(|| PhysicsError::invalid(id, "payload is not loaded"))?;
        let available = mass.to(Unit::Kilogram)?.value;
        if release > available {
            return Err(PhysicsError::invalid(
                &stage.id,
                format!("release {release} kg exceeds {id} payload {available} kg"),
            ));
        }
        *mass = Quantity::new(available - release, Unit::Kilogram)?;
        released.insert(id.clone(), release);
    }
    Ok(released)
}
