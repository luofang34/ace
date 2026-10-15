use crate::{Trace, evidence};
use ace_engine::{
    EngineResult, Margin, MarginStatus, Snapshot, ValueStatus, evaluate_requirements,
};
use ace_model::VehicleModel;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Structured output of one analysis, retaining its current validity separately.
#[derive(Debug, Clone, Serialize)]
pub struct DomainPanel {
    /// Analysis identifier.
    pub analysis: String,
    /// Declared plugin contract for interpreting the domain result.
    pub plugin: String,
    /// Current output validity; stale details are historical, never current results.
    pub status: ValueStatus,
    /// Whether the displayed detail belongs to an earlier calculation.
    pub historical: bool,
    /// Exact producing run for the displayed evidence.
    pub run_id: String,
    /// Domain-defined geometry, loading, mission, or cost evidence.
    pub details: serde_json::Value,
}

/// One-page projection of an authoritative model revision.
#[derive(Debug, Clone, Serialize)]
pub struct Sheet {
    /// Separate execution, mandatory feasibility and evidence quality.
    pub assessment: ace_engine::Assessment,
    /// Browser-safe revision token, including the full u64 range.
    pub revision: String,
    /// Authoritative declarations, including units, ownership, and uncertainty.
    pub model: VehicleModel,
    /// Current values and their complete reachable calculation evidence.
    pub snapshot: Snapshot,
    /// Computable requirement margins.
    pub margins: BTreeMap<String, Margin>,
    /// Requirements ranked by violation, missing evidence, and active margin.
    pub attention: Vec<String>,
    /// Current loading and section witnesses for automatic constraint navigation.
    pub focus: BTreeMap<String, crate::ConstraintFocus>,
    /// Independent input identifiers accepted by transactional edits.
    pub editable: BTreeSet<String>,
    /// Structured results with explicit current/historical quality.
    pub panels: Vec<DomainPanel>,
    /// Navigation from requirements to calculations and back to geometry.
    pub trace: Trace,
}

/// Build a sheet without running or changing the design model.
pub fn project(model: &VehicleModel, snapshot: &Snapshot) -> EngineResult<Sheet> {
    let margins = evaluate_requirements(model, snapshot)?;
    let panels = panels(model, snapshot);
    let snapshot = evidence::reachable(snapshot, &panels);
    let trace = crate::trace::build(model, &snapshot, &margins)?;
    let focus = crate::focus::build(model, &snapshot, &margins, &panels);
    let produced: BTreeSet<_> = model
        .analyses
        .values()
        .flat_map(|analysis| analysis.outputs.values())
        .collect();
    let editable = model
        .parameters
        .iter()
        .filter(|(name, value)| value.expression.is_none() && !produced.contains(name))
        .map(|(name, _)| name.clone())
        .collect();
    let mut attention: Vec<_> = margins.keys().cloned().collect();
    attention.sort_by(|a, b| {
        rank(margins.get(a))
            .cmp(&rank(margins.get(b)))
            .then_with(|| slack(margins.get(a)).total_cmp(&slack(margins.get(b))))
            .then(a.cmp(b))
    });
    Ok(Sheet {
        assessment: ace_engine::assess(model, &snapshot)?,
        revision: model.revision.to_string(),
        model: model.clone(),
        snapshot,
        margins,
        attention,
        focus,
        editable,
        panels,
        trace,
    })
}

fn panels(model: &VehicleModel, snapshot: &Snapshot) -> Vec<DomainPanel> {
    model
        .analyses
        .iter()
        .filter_map(|(id, analysis)| {
            let node = format!("analysis:{id}");
            let run = snapshot
                .runs
                .iter()
                .rev()
                .find(|run| run.node == node && !run.details.is_null())?;
            let status = analysis
                .outputs
                .values()
                .filter_map(|name| snapshot.values.get(name))
                .map(|value| value.status)
                .next()
                .unwrap_or(ValueStatus::Unavailable);
            let historical = !analysis
                .outputs
                .values()
                .all(|name| current_candidate(snapshot, name, &run.id));
            Some(DomainPanel {
                analysis: id.clone(),
                plugin: analysis.plugin.clone(),
                status,
                historical,
                run_id: run.id.clone(),
                details: run.details.clone(),
            })
        })
        .collect()
}

fn current_candidate(snapshot: &Snapshot, name: &str, expected: &str) -> bool {
    let mut value = snapshot.values.get(name);
    for _ in 0..=snapshot.runs.len() {
        let Some(record) = value.filter(|value| value.usable()) else {
            return false;
        };
        let Some(id) = &record.run_id else {
            return false;
        };
        if id == expected {
            return true;
        }
        let Some(run) = snapshot.runs.iter().find(|run| &run.id == id) else {
            return false;
        };
        if !run.node.starts_with("closure:") {
            return false;
        }
        // Only this iteration's candidate supports the current closure; previous inputs are history.
        value = run.inputs.get(&format!("candidate:{name}"));
    }
    false
}

fn rank(margin: Option<&Margin>) -> u8 {
    match margin.map(|margin| margin.status) {
        Some(MarginStatus::Fail) => 0,
        Some(MarginStatus::Unknown) | None => 1,
        Some(MarginStatus::Warn) => 2,
        Some(MarginStatus::Pass) => 3,
        Some(MarginStatus::NotApplicable) => 4,
    }
}

fn slack(margin: Option<&Margin>) -> f64 {
    margin
        .and_then(|margin| {
            margin
                .fraction
                .or_else(|| margin.absolute.map(|q| q.value.signum()))
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
