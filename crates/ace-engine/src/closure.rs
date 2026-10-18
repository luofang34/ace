use crate::execution::Executor;
use crate::graph::{Component, Graph};
use crate::record::finalize;
use crate::{ClosureResult, EngineResult, RunRecord, ValueRecord, ValueStatus};
use ace_model::{Closure, ModelError, Quantity};
use std::collections::BTreeMap;

pub(crate) struct ClosureExecution {
    pub values: BTreeMap<String, ValueRecord>,
    pub runs: Vec<RunRecord>,
    pub result: ClosureResult,
}

pub(crate) fn solve(
    id: &str,
    component: &Component,
    graph: &Graph,
    executor: &Executor<'_>,
    values: &BTreeMap<String, ValueRecord>,
) -> EngineResult<ClosureExecution> {
    let closure = executor
        .model
        .closures
        .get(id)
        .ok_or_else(|| ModelError::UnknownReference(id.into()))?;
    let mut working = values.clone();
    seed(closure, executor, &mut working)?;
    let mut outcome = ClosureExecution {
        values: BTreeMap::new(),
        runs: Vec::new(),
        result: ClosureResult {
            id: id.into(),
            converged: false,
            iterations: 0,
            residuals: BTreeMap::new(),
            message: None,
        },
    };
    for iteration in 1..=closure.max_iterations {
        executor.registry.control().checkpoint()?;
        outcome.result.iterations = iteration;
        let mut candidates = BTreeMap::new();
        for name in &component.nodes {
            executor.registry.control().checkpoint()?;
            let node = graph
                .nodes
                .get(name)
                .ok_or_else(|| ModelError::UnknownReference(name.clone()))?;
            let result = executor.execute(node, &working, Some(iteration))?;
            candidates.extend(result.values);
            outcome.runs.push(result.run);
        }
        if candidates.values().any(|value| !value.usable()) {
            outcome.values = invalidate(
                &candidates,
                ValueStatus::Blocked,
                "closure has unavailable or failed outputs",
            );
            outcome.result.message = Some("closure has unavailable or failed outputs".into());
            return Ok(outcome);
        }
        let converged = residuals(
            closure,
            &working,
            &candidates,
            &mut outcome.result.residuals,
        )?;
        let (records, run) = relax(id, closure, executor, &working, &candidates, iteration)?;
        working.extend(records.clone());
        outcome.values = records;
        outcome.runs.push(run);
        if converged {
            outcome.result.converged = true;
            return Ok(outcome);
        }
    }
    let message = format!(
        "closure {id} did not converge within {} iterations",
        closure.max_iterations
    );
    outcome.values = invalidate(&outcome.values, ValueStatus::Nonconverged, &message);
    outcome.result.message = Some(message);
    Ok(outcome)
}

fn seed(
    closure: &Closure,
    executor: &Executor<'_>,
    working: &mut BTreeMap<String, ValueRecord>,
) -> EngineResult<()> {
    for name in closure.tolerances.keys() {
        let parameter = executor.model.parameter(name)?;
        let quantity = parameter
            .value
            .ok_or_else(|| ModelError::invalid(name, "missing closure seed"))?
            .to(parameter.unit)?;
        working.insert(
            name.clone(),
            ValueRecord {
                parameter: name.clone(),
                status: ValueStatus::Input,
                quantity: Some(quantity),
                run_id: None,
                revision: parameter.revision,
                source: parameter.source.clone(),
                message: Some("closure initial seed".into()),
            },
        );
    }
    Ok(())
}

fn quantity(values: &BTreeMap<String, ValueRecord>, name: &str) -> EngineResult<Quantity> {
    values
        .get(name)
        .and_then(|value| value.quantity)
        .ok_or_else(|| ModelError::invalid(name, "missing closure value").into())
}

fn residuals(
    closure: &Closure,
    working: &BTreeMap<String, ValueRecord>,
    candidates: &BTreeMap<String, ValueRecord>,
    residuals: &mut BTreeMap<String, Quantity>,
) -> EngineResult<bool> {
    let mut converged = true;
    for (name, tolerance) in &closure.tolerances {
        let old = quantity(working, name)?;
        let new = quantity(candidates, name)?.to(old.unit)?;
        let residual = (old.value - new.value).abs();
        residuals.insert(name.clone(), Quantity::new(residual, old.unit)?);
        converged &= residual
            <= tolerance.absolute.to(old.unit)?.value + tolerance.relative * new.value.abs();
    }
    Ok(converged)
}

fn relax(
    id: &str,
    closure: &Closure,
    executor: &Executor<'_>,
    previous: &BTreeMap<String, ValueRecord>,
    candidates: &BTreeMap<String, ValueRecord>,
    iteration: u32,
) -> EngineResult<(BTreeMap<String, ValueRecord>, RunRecord)> {
    let status = if candidates
        .values()
        .any(|value| value.status == ValueStatus::Approximate)
    {
        ValueStatus::Approximate
    } else {
        ValueStatus::Computed
    };
    let mut inputs = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    for (name, value) in candidates {
        let old = quantity(previous, name)?;
        let new = quantity(candidates, name)?.to(old.unit)?;
        outputs.insert(
            name.clone(),
            Quantity::new(
                old.value + closure.relaxation * (new.value - old.value),
                old.unit,
            )?,
        );
        inputs.insert(format!("candidate:{name}"), value.clone());
        if let Some(value) = previous.get(name) {
            inputs.insert(format!("previous:{name}"), value.clone());
        }
    }
    let run = finalize(RunRecord {
        id: String::new(),
        node: format!("closure:{id}"),
        model_digest: executor.digest.into(),
        plugin: None,
        contract: None,
        calculation: serde_json::to_value(closure)?,
        inputs,
        outputs,
        status,
        resources: Default::default(),
        details: serde_json::Value::Null,
        artifacts: Vec::new(),
        message: None,
        iteration: Some(iteration),
    })?;
    let mut records = candidates.clone();
    for (name, record) in &mut records {
        record.quantity = run.outputs.get(name).copied();
        record.run_id = Some(run.id.clone());
        record.status = status;
    }
    Ok((records, run))
}

fn invalidate(
    values: &BTreeMap<String, ValueRecord>,
    status: ValueStatus,
    message: &str,
) -> BTreeMap<String, ValueRecord> {
    values
        .iter()
        .map(|(name, value)| {
            let mut value = value.clone();
            value.status = status;
            value.quantity = None;
            value.message = Some(message.into());
            (name.clone(), value)
        })
        .collect()
}
