use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::archive::StudyArchiveDraft;
use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, OptimizerCheckpoint, StudyArchive, StudyArchiveWorkflow,
    StudyRunResult,
};
use crate::domain::study::StudyDefinition;
use crate::services::analysis::ApplicationService;
use crate::services::study::evaluation::{
    candidate_descriptor, evaluate_candidate_blocking, evaluator_signature,
};
use crate::services::study::loading::PreparedStudy;
use crate::services::study::ranking;

struct WorkingStudy {
    study_id: String,
    study_digest: String,
    baseline_digest: String,
    evaluator_signature: String,
    candidates: Vec<CandidateDescriptor>,
    evaluation_ids: Vec<String>,
    selected_candidate_ids: Vec<String>,
    workflow: StudyArchiveWorkflow,
}

impl WorkingStudy {
    fn new(prepared: &PreparedStudy) -> Self {
        Self {
            study_id: prepared.document.study.id.clone(),
            study_digest: prepared.study_digest.clone(),
            baseline_digest: prepared.baseline_digest.clone(),
            evaluator_signature: evaluator_signature(),
            candidates: Vec::new(),
            evaluation_ids: Vec::new(),
            selected_candidate_ids: Vec::new(),
            workflow: StudyArchiveWorkflow {
                checkpoint: OptimizerCheckpoint {
                    generation: 0,
                    rng_state: prepared.document.study.search.seed,
                    population: Vec::new(),
                },
                ..Default::default()
            },
        }
    }

    fn from_archive(archive: StudyArchive) -> Self {
        Self {
            study_id: archive.study_id,
            study_digest: archive.study_digest,
            baseline_digest: archive.baseline_digest,
            evaluator_signature: archive.evaluator_signature,
            candidates: archive.candidates,
            evaluation_ids: archive.evaluation_ids,
            selected_candidate_ids: archive.selected_candidate_ids,
            workflow: archive.workflow,
        }
    }

    fn archive(&self, complete: bool) -> AexResult<StudyArchive> {
        StudyArchive::from_draft(StudyArchiveDraft {
            study_id: self.study_id.clone(),
            study_digest: self.study_digest.clone(),
            baseline_digest: self.baseline_digest.clone(),
            evaluator_signature: self.evaluator_signature.clone(),
            complete,
            candidates: self.candidates.clone(),
            evaluation_ids: self.evaluation_ids.clone(),
            selected_candidate_ids: self.selected_candidate_ids.clone(),
            workflow: self.workflow.clone(),
        })
    }
}

pub(super) fn run_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
) -> AexResult<StudyRunResult> {
    let existing = matching_archive(service, prepared)?;
    let reused = existing
        .as_ref()
        .map_or(0, |archive| archive.workflow.outcomes.len());
    if let Some(archive) = existing.as_ref().filter(|archive| archive.complete) {
        return ranking::run_result(service, archive, reused, 10);
    }
    let mut state =
        existing.map_or_else(|| WorkingStudy::new(prepared), WorkingStudy::from_archive);
    let archive = match prepared.document.study.search.strategy.as_str() {
        "grid" => run_grid_blocking(service, prepared, &mut state)?,
        "evolutionary" => run_evolutionary_blocking(service, prepared, &mut state)?,
        strategy => {
            return Err(AexError::validation(
                "UNSUPPORTED_STUDY_SEARCH",
                "study.search.strategy",
                strategy,
            ));
        }
    };
    ranking::run_result(service, &archive, reused, 10)
}

pub(super) fn latest_archive_for_study(
    service: &ApplicationService,
    study_id: &str,
) -> AexResult<StudyArchive> {
    service
        .studies
        .list_archives_blocking()?
        .into_iter()
        .filter(|archive| archive.study_id == study_id)
        .max_by(compare_progress)
        .ok_or_else(|| {
            AexError::validation(
                "STUDY_ARCHIVE_NOT_FOUND",
                "study_id",
                format!("no local archive exists for {study_id}"),
            )
        })
}

fn matching_archive(
    service: &ApplicationService,
    prepared: &PreparedStudy,
) -> AexResult<Option<StudyArchive>> {
    let signature = evaluator_signature();
    Ok(service
        .studies
        .list_archives_blocking()?
        .into_iter()
        .filter(|archive| {
            archive.study_id == prepared.document.study.id
                && archive.study_digest == prepared.study_digest
                && archive.baseline_digest == prepared.baseline_digest
                && archive.evaluator_signature == signature
        })
        .max_by(compare_progress))
}

fn compare_progress(left: &StudyArchive, right: &StudyArchive) -> Ordering {
    left.complete
        .cmp(&right.complete)
        .then_with(|| {
            left.workflow
                .outcomes
                .len()
                .cmp(&right.workflow.outcomes.len())
        })
        .then_with(|| {
            left.workflow
                .checkpoint
                .generation
                .cmp(&right.workflow.checkpoint.generation)
        })
        .then_with(|| left.archive_id.cmp(&right.archive_id))
}

fn run_grid_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    state: &mut WorkingStudy,
) -> AexResult<StudyArchive> {
    let maximum = search_limit(prepared.document.study.search.max_evaluations)?;
    let genes = grid_genes(&prepared.document.study, maximum);
    for values in genes {
        evaluate_if_new(service, prepared, state, &values, 0)?;
        persist_snapshot(service, prepared, state, false)?;
    }
    state.workflow.checkpoint.population.clear();
    persist_snapshot(service, prepared, state, true)
}

fn run_evolutionary_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    state: &mut WorkingStudy,
) -> AexResult<StudyArchive> {
    let search = &prepared.document.study.search;
    let population_size = search_limit(search.population)?;
    let maximum = search_limit(search.max_evaluations)?;
    let mut rng = DeterministicRng::new(state.workflow.checkpoint.rng_state);
    let mut population = if state.workflow.checkpoint.population.is_empty() {
        initial_population(&prepared.document.study, population_size, &mut rng)
    } else {
        state.workflow.checkpoint.population.clone()
    };
    while state.workflow.checkpoint.generation < search.generations
        && state.workflow.outcomes.len() < maximum
    {
        let generation = state.workflow.checkpoint.generation;
        state.workflow.checkpoint.population = population.clone();
        evaluate_population(service, prepared, state, &population, generation, maximum)?;
        ranking::rank(
            &prepared.document.study,
            &mut state.workflow,
            &mut state.selected_candidate_ids,
        );
        population = breed_population(
            prepared,
            state,
            &population,
            population_size,
            search.mutation_rate,
            &mut rng,
        )?;
        state.workflow.checkpoint.generation = state.workflow.checkpoint.generation.wrapping_add(1);
        state.workflow.checkpoint.rng_state = rng.state;
        state.workflow.checkpoint.population = population.clone();
        persist_snapshot(service, prepared, state, false)?;
    }
    let complete = state.workflow.checkpoint.generation >= search.generations
        || state.workflow.outcomes.len() >= maximum;
    persist_snapshot(service, prepared, state, complete)
}

fn evaluate_population(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    state: &mut WorkingStudy,
    population: &[BTreeMap<String, String>],
    generation: u32,
    maximum: usize,
) -> AexResult<()> {
    for genes in population {
        if state.workflow.outcomes.len() >= maximum {
            break;
        }
        if evaluate_if_new(service, prepared, state, genes, generation)? {
            persist_snapshot(service, prepared, state, false)?;
        }
    }
    Ok(())
}

fn evaluate_if_new(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    state: &mut WorkingStudy,
    genes: &BTreeMap<String, String>,
    generation: u32,
) -> AexResult<bool> {
    let candidate = candidate_descriptor(prepared, genes)?;
    if state
        .candidates
        .iter()
        .any(|known| known.candidate_id == candidate.candidate_id)
    {
        return Ok(false);
    }
    let evaluated = evaluate_candidate_blocking(service, prepared, candidate, generation)?;
    service
        .studies
        .save_evaluation_blocking(&evaluated.evidence)?;
    state
        .evaluation_ids
        .push(evaluated.evidence.evaluation_id.clone());
    state.workflow.outcomes.push(evaluated.outcome);
    state.candidates.push(evaluated.candidate);
    Ok(true)
}

fn persist_snapshot(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    state: &mut WorkingStudy,
    complete: bool,
) -> AexResult<StudyArchive> {
    ranking::rank(
        &prepared.document.study,
        &mut state.workflow,
        &mut state.selected_candidate_ids,
    );
    let archive = state.archive(complete)?;
    service.studies.save_archive_blocking(&archive)?;
    Ok(archive)
}

fn grid_genes(study: &StudyDefinition, maximum: usize) -> Vec<BTreeMap<String, String>> {
    let mut rows = vec![BTreeMap::new()];
    for variable in &study.variables {
        let mut expanded = Vec::new();
        for row in &rows {
            for value in &variable.values {
                let mut next = row.clone();
                next.insert(variable.id.clone(), value.clone());
                expanded.push(next);
                if expanded.len() >= maximum {
                    break;
                }
            }
            if expanded.len() >= maximum {
                break;
            }
        }
        rows = expanded;
    }
    rows.truncate(maximum);
    rows
}

fn initial_population(
    study: &StudyDefinition,
    size: usize,
    rng: &mut DeterministicRng,
) -> Vec<BTreeMap<String, String>> {
    let mut population = grid_genes(study, size);
    while population.len() < size {
        population.push(random_genes(study, rng));
    }
    population
}

fn breed_population(
    prepared: &PreparedStudy,
    state: &WorkingStudy,
    population: &[BTreeMap<String, String>],
    size: usize,
    mutation_rate: f64,
    rng: &mut DeterministicRng,
) -> AexResult<Vec<BTreeMap<String, String>>> {
    let mut ranked = population.to_vec();
    ranked.sort_by(|left, right| {
        compare_optional(
            population_outcome(prepared, state, left),
            population_outcome(prepared, state, right),
        )
    });
    let elite_count = (size / 4).clamp(1, ranked.len().max(1));
    let elites = ranked.into_iter().take(elite_count).collect::<Vec<_>>();
    let mut next = elites.clone();
    let mut seen = encoded_genes(&next)?;
    let maximum_attempts = size.saturating_mul(20);
    let mut attempts = 0_usize;
    while next.len() < size && attempts < maximum_attempts {
        attempts = attempts.wrapping_add(1);
        let first = &elites[rng.index(elites.len())];
        let second = &elites[rng.index(elites.len())];
        let child = crossover_and_mutate(study(prepared), first, second, mutation_rate, rng);
        let encoded = serde_json::to_string(&child).map_err(|source| AexError::Json { source })?;
        if seen.insert(encoded) {
            next.push(child);
        }
    }
    while next.len() < size {
        next.push(random_genes(study(prepared), rng));
    }
    Ok(next)
}

fn study(prepared: &PreparedStudy) -> &StudyDefinition {
    &prepared.document.study
}

fn population_outcome<'a>(
    prepared: &PreparedStudy,
    state: &'a WorkingStudy,
    genes: &BTreeMap<String, String>,
) -> Option<&'a CandidateOutcome> {
    candidate_descriptor(prepared, genes)
        .ok()
        .and_then(|candidate| {
            state
                .workflow
                .outcomes
                .iter()
                .find(|outcome| outcome.candidate_id == candidate.candidate_id)
        })
}

fn compare_optional(left: Option<&CandidateOutcome>, right: Option<&CandidateOutcome>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => ranking::compare(left, right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn crossover_and_mutate(
    study: &StudyDefinition,
    first: &BTreeMap<String, String>,
    second: &BTreeMap<String, String>,
    mutation_rate: f64,
    rng: &mut DeterministicRng,
) -> BTreeMap<String, String> {
    study
        .variables
        .iter()
        .map(|variable| {
            let inherited = if rng.index(2) == 0 {
                first.get(&variable.id)
            } else {
                second.get(&variable.id)
            };
            let value = if rng.probability() < mutation_rate {
                &variable.values[rng.index(variable.values.len())]
            } else {
                inherited.unwrap_or(&variable.values[0])
            };
            (variable.id.clone(), value.clone())
        })
        .collect()
}

fn random_genes(study: &StudyDefinition, rng: &mut DeterministicRng) -> BTreeMap<String, String> {
    study
        .variables
        .iter()
        .map(|variable| {
            (
                variable.id.clone(),
                variable.values[rng.index(variable.values.len())].clone(),
            )
        })
        .collect()
}

fn encoded_genes(population: &[BTreeMap<String, String>]) -> AexResult<BTreeSet<String>> {
    population
        .iter()
        .map(|genes| serde_json::to_string(genes).map_err(|source| AexError::Json { source }))
        .collect()
}

fn search_limit(value: u32) -> AexResult<usize> {
    usize::try_from(value)
        .map_err(|source| AexError::analysis("STUDY_SEARCH_LIMIT_TOO_LARGE", source.to_string()))
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(state: u64) -> Self {
        Self {
            state: if state == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                state
            },
        }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn index(&mut self, length: usize) -> usize {
        if length == 0 {
            return 0;
        }
        let length_u64 = u64::try_from(length).unwrap_or(u64::MAX);
        usize::try_from(self.next() % length_u64).unwrap_or(0)
    }

    fn probability(&mut self) -> f64 {
        (self.next() >> 11) as f64 / ((1_u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests;
