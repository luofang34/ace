use crate::DomainPanel;
use ace_engine::Snapshot;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn reachable(snapshot: &Snapshot, panels: &[DomainPanel]) -> Snapshot {
    let runs: BTreeMap<_, _> = snapshot.runs.iter().map(|run| (&run.id, run)).collect();
    let mut pending: Vec<_> = snapshot
        .values
        .values()
        .filter_map(|value| value.run_id.as_ref())
        .chain(panels.iter().map(|panel| &panel.run_id))
        .collect();
    let mut needed = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !needed.insert(id) {
            continue;
        }
        if let Some(run) = runs.get(id) {
            pending.extend(
                run.inputs
                    .values()
                    .filter_map(|value| value.run_id.as_ref()),
            );
        }
    }
    Snapshot {
        runs: snapshot
            .runs
            .iter()
            .filter(|run| needed.contains(&run.id))
            .cloned()
            .collect(),
        ..snapshot.clone()
    }
}
