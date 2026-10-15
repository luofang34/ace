use crate::{PhysicsError, PhysicsResult};
use ace_engine::{AnalysisContext, PhaseIncrement, PhaseRun};
use ace_model::{AnalysisMode, MissionStage, Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

pub(super) fn solve(
    stage: &MissionStage,
    state: &VehicleState,
    context: &AnalysisContext<'_>,
) -> PhysicsResult<(Vec<PhaseIncrement>, Option<PhaseRun>)> {
    let analysis = context
        .resources
        .stage_analyses
        .get(&stage.analysis)
        .ok_or_else(|| PhysicsError::invalid(&stage.id, "phase analysis is unavailable"))?;
    if analysis.mode == AnalysisMode::Phase {
        let run = context
            .registry
            .evaluate_phase(analysis, state, context)
            .map_err(|source| PhysicsError::Phase {
                stage: stage.id.clone(),
                source,
            })?;
        return Ok((run.output.increments.clone(), Some(run)));
    }
    let output = |port: &str, unit: Unit| -> PhysicsResult<Quantity> {
        let parameter = analysis.outputs.get(port).ok_or_else(|| {
            PhysicsError::invalid(&stage.id, format!("missing phase output {port}"))
        })?;
        context
            .values
            .get(parameter)
            .ok_or_else(|| {
                PhysicsError::invalid(&stage.id, format!("missing phase result {parameter}"))
            })?
            .to(unit)
            .map_err(Into::into)
    };
    let mut conditions = BTreeMap::new();
    for (port, parameter) in &analysis.outputs {
        if let Some(id) = port.strip_prefix("state.") {
            let value = context.values.get(parameter).ok_or_else(|| {
                PhysicsError::invalid(&stage.id, format!("missing state result {parameter}"))
            })?;
            conditions.insert(id.into(), *value);
        }
    }
    Ok((
        vec![PhaseIncrement {
            duration: output("duration", Unit::Second)?,
            distance: output("distance", Unit::Meter)?,
            fuel_burn: output("fuel_burn", Unit::Kilogram)?,
            cost: output("cost", Unit::Dollar)?,
            conditions,
            details: serde_json::Value::Null,
        }],
        None,
    ))
}
