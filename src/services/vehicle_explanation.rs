use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use ace_engine::{Margin, RunRecord, ValueRecord};
use ace_model::Parameter;
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};

/// Largest number of producer levels an explanation walks upstream.
pub(crate) const MAXIMUM_DEPTH: u32 = 16;

/// Why a vehicle parameter has its current value, from the frozen calculation evidence.
#[derive(Debug, Serialize)]
pub(crate) struct Explanation {
    pub parameter: String,
    pub declaration: Parameter,
    pub value: ValueRecord,
    /// Calculation that produced the value; absent for independent inputs.
    pub calculation: Option<RunRecord>,
    /// Producers of the calculation's inputs, nearest first, within the requested depth.
    pub upstream: Vec<UpstreamStep>,
    /// Analyses and expressions that read this parameter.
    pub consumers: Consumers,
    /// Requirement margins evaluated on this parameter.
    pub margins: BTreeMap<String, Margin>,
}

/// One upstream producer, summarized; its full record is retrievable by explaining `parameter`.
#[derive(Debug, Serialize)]
pub(crate) struct UpstreamStep {
    pub depth: u32,
    pub parameter: String,
    pub run_id: String,
    pub node: String,
    pub plugin: Option<String>,
    pub status: ace_engine::ValueStatus,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Consumers {
    pub analyses: Vec<String>,
    pub expressions: Vec<String>,
}

/// Synthesize the vehicle and explain `parameter`, walking at most `depth` producer levels.
pub(crate) fn explain_blocking(
    path: &Path,
    overrides: &BTreeMap<String, String>,
    parameter: &str,
    depth: u32,
) -> AexResult<Explanation> {
    if depth > MAXIMUM_DEPTH {
        return Err(AexError::validation(
            "INVALID_EXPLANATION_DEPTH",
            "depth",
            format!("depth must not exceed {MAXIMUM_DEPTH}"),
        ));
    }
    let mut session = super::synthesis::open_blocking(path, overrides)?;
    let declaration = session
        .model()
        .parameter(parameter)
        .map_err(|_| {
            AexError::validation(
                "UNKNOWN_EXPLANATION_PARAMETER",
                parameter.to_owned(),
                format!("{} declares no such parameter", path.display()),
            )
        })?
        .clone();
    let snapshot = session.run()?;
    let runs: BTreeMap<&str, &RunRecord> = snapshot
        .runs
        .iter()
        .map(|run| (run.id.as_str(), run))
        .collect();
    let value = snapshot.values.get(parameter).cloned().ok_or_else(|| {
        AexError::analysis("MISSING_EXPLANATION_VALUE", "synthesis recorded no value")
    })?;
    let calculation = value
        .run_id
        .as_deref()
        .and_then(|id| runs.get(id))
        .map(|run| (*run).clone());
    let upstream = calculation
        .as_ref()
        .map(|run| walk_upstream(run, &runs, depth))
        .unwrap_or_default();
    let margins = ace_engine::evaluate_requirements(session.model(), &snapshot)?
        .into_iter()
        .filter(|(_, margin)| margin.parameter == parameter)
        .collect();
    Ok(Explanation {
        parameter: parameter.to_owned(),
        consumers: consumers(session.model(), parameter),
        declaration,
        value,
        calculation,
        upstream,
        margins,
    })
}

fn walk_upstream(
    start: &RunRecord,
    runs: &BTreeMap<&str, &RunRecord>,
    depth: u32,
) -> Vec<UpstreamStep> {
    let mut steps = Vec::new();
    let mut visited = BTreeSet::from([start.id.clone()]);
    let mut queue = VecDeque::from([(start, 1_u32)]);
    while let Some((run, level)) = queue.pop_front() {
        if level > depth {
            continue;
        }
        for input in run.inputs.values() {
            let Some(producer) = input.run_id.as_deref().and_then(|id| runs.get(id)) else {
                continue;
            };
            if !visited.insert(producer.id.clone()) {
                continue;
            }
            steps.push(UpstreamStep {
                depth: level,
                parameter: input.parameter.clone(),
                run_id: producer.id.clone(),
                node: producer.node.clone(),
                plugin: producer.plugin.as_ref().map(|plugin| plugin.id.clone()),
                status: producer.status,
            });
            queue.push_back((producer, level.saturating_add(1)));
        }
    }
    steps
}

fn consumers(model: &ace_model::VehicleModel, parameter: &str) -> Consumers {
    let analyses = model
        .analyses
        .iter()
        .filter(|(_, analysis)| analysis.inputs.values().any(|input| input == parameter))
        .map(|(id, _)| id.clone())
        .collect();
    let expressions = model
        .parameters
        .iter()
        .filter(|(_, declared)| {
            declared.expression.as_ref().is_some_and(|expression| {
                expression
                    .dependencies()
                    .is_ok_and(|dependencies| dependencies.contains(parameter))
            })
        })
        .map(|(name, _)| name.clone())
        .collect();
    Consumers {
        analyses,
        expressions,
    }
}
