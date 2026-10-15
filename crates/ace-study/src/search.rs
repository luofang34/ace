use crate::{SearchMethod, StopReason, StudyArchive, StudyPlan, StudyResult};
use ace_engine::Session;
use ace_model::Quantity;
use std::collections::{BTreeMap, BTreeSet};

mod grid;
mod refine;

/// Evaluate a bounded design study without changing the caller's session.
/// Unaffected calculations are reused from a private, current baseline fork.
pub fn run(session: &Session, plan: &StudyPlan) -> StudyResult<StudyArchive> {
    plan.validate(session.model())?;
    let mut baseline = session.fork();
    baseline.run()?;
    let algorithm_revision = crate::record::algorithm_revision();
    let request_id = crate::record::digest(&(session.model(), plan, &algorithm_revision))?;
    let mut search = Search {
        baseline,
        seen: BTreeSet::new(),
        archive: StudyArchive {
            id: String::new(),
            request_id,
            algorithm_revision,
            baseline: session.model().clone(),
            plan: plan.clone(),
            candidates: Vec::new(),
            evidence: BTreeMap::new(),
            pareto: Vec::new(),
            provisional_pareto: Vec::new(),
            stop_reason: StopReason::EvaluationBudget,
        },
    };
    search.archive.stop_reason = match &plan.method {
        SearchMethod::Grid { axes } => grid::run(&mut search, axes)?,
        SearchMethod::Refine { levels, variables } => refine::run(&mut search, *levels, variables)?,
    };
    search.archive.pareto =
        crate::ranking::front(&search.archive.candidates, crate::Feasibility::Satisfied)?;
    search.archive.provisional_pareto =
        crate::ranking::front(&search.archive.candidates, crate::Feasibility::Provisional)?;
    search.archive.id = crate::record::digest(&search.archive)?;
    Ok(search.archive)
}

struct Search {
    baseline: Session,
    archive: StudyArchive,
    seen: BTreeSet<String>,
}

impl Search {
    fn consider(&mut self, updates: BTreeMap<String, Quantity>) -> StudyResult<bool> {
        let id = crate::record::digest(&updates)?;
        if self.seen.contains(&id) {
            return Ok(true);
        }
        if self.archive.candidates.len() as u64 >= u64::from(self.archive.plan.max_evaluations) {
            return Ok(false);
        }
        let candidate = crate::evaluation::evaluate(
            &self.baseline,
            updates,
            &self.archive.request_id,
            &mut self.archive.evidence,
        )?;
        self.archive.candidates.push(candidate);
        self.seen.insert(id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
