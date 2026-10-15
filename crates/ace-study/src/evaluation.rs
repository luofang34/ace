use crate::{Candidate, CandidateState, ObjectiveValue, StudyResult};
use ace_engine::{RunRecord, Session, evaluate_requirements};
use ace_model::{Quantity, Source};
use std::collections::BTreeMap;

pub(crate) fn evaluate(
    baseline: &Session,
    updates: BTreeMap<String, Quantity>,
    request_id: &str,
    evidence: &mut BTreeMap<String, RunRecord>,
) -> StudyResult<Candidate> {
    let mut session = baseline.fork();
    session.edit(
        &updates,
        Source {
            kind: "study".into(),
            reference: format!("vehicle-study:{request_id}"),
            revision: None,
        },
    )?;
    let snapshot = session.run()?;
    let margins = evaluate_requirements(session.model(), &snapshot)?;
    let objectives = session
        .model()
        .objectives
        .iter()
        .filter_map(|(id, objective)| {
            snapshot.values.get(&objective.metric).map(|value| {
                (
                    id.clone(),
                    ObjectiveValue {
                        direction: objective.direction,
                        value: value.clone(),
                    },
                )
            })
        })
        .collect();
    let feasibility = ace_engine::assess(session.model(), &snapshot)?.feasibility;
    let run_ids = crate::evidence::collect(&snapshot, evidence);
    let state = CandidateState {
        build: snapshot.build,
        model_digest: snapshot.model_digest,
        values: snapshot.values,
        closures: snapshot.closures,
        executed: snapshot.executed,
        run_ids,
    };
    let mut candidate = Candidate {
        id: String::new(),
        updates,
        model: session.model().clone(),
        state,
        margins,
        objectives,
        feasibility,
    };
    candidate.id = crate::record::digest(&candidate)?;
    Ok(candidate)
}
