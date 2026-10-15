use ace_engine::{RunRecord, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn collect(
    snapshot: &Snapshot,
    archive: &mut BTreeMap<String, RunRecord>,
) -> Vec<String> {
    let runs: BTreeMap<_, _> = snapshot
        .runs
        .iter()
        .map(|run| (run.id.as_str(), run))
        .collect();
    let mut pending: Vec<_> = snapshot
        .values
        .values()
        .filter_map(|value| value.run_id.clone())
        .collect();
    for node in &snapshot.executed {
        if let Some(run) = snapshot.runs.iter().rev().find(|run| &run.node == node) {
            pending.push(run.id.clone());
        }
    }
    let mut selected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !selected.insert(id.clone()) {
            continue;
        }
        if let Some(run) = runs.get(id.as_str()) {
            pending.extend(run.inputs.values().filter_map(|value| value.run_id.clone()));
            archive.entry(id).or_insert_with(|| (*run).clone());
        }
    }
    // Keep execution order so sheet panels choose the actual final calculation.
    snapshot
        .runs
        .iter()
        .filter(|run| selected.remove(&run.id))
        .map(|run| run.id.clone())
        .collect()
}
