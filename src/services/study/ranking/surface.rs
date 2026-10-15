use std::collections::{BTreeMap, BTreeSet};

use crate::domain::evidence::{CandidateOutcome, StudyArchive, StudyTradeSurface};

pub(super) fn from_archive(archive: &StudyArchive) -> Option<StudyTradeSurface> {
    if archive.candidates.is_empty()
        || archive.workflow.checkpoint.generation != 0
        || archive
            .workflow
            .outcomes
            .iter()
            .any(|outcome| outcome.generation != 0)
    {
        return None;
    }
    let paths = varying_paths(archive)?;
    let x_parsed = parsed_axis(archive, &paths[0])?;
    let y_parsed = parsed_axis(archive, &paths[1])?;
    let x_values = unique_values(&x_parsed);
    let y_values = unique_values(&y_parsed);
    let expected = x_values.len().checked_mul(y_values.len())?;
    if expected != archive.candidates.len() {
        return None;
    }
    let outcomes = archive
        .workflow
        .outcomes
        .iter()
        .map(|outcome| (outcome.candidate_id.as_str(), outcome))
        .collect::<BTreeMap<_, _>>();
    let objective_id = archive
        .workflow
        .outcomes
        .first()?
        .objective_values
        .keys()
        .next()?
        .clone();
    let (values, feasible_mask) = row_major_values(
        archive,
        &outcomes,
        &paths,
        &x_values,
        &y_values,
        &objective_id,
    )?;
    Some(StudyTradeSurface {
        x_path: paths[0].clone(),
        x_unit: common_unit(&x_parsed)?,
        x_values,
        y_path: paths[1].clone(),
        y_unit: common_unit(&y_parsed)?,
        y_values,
        objective_id,
        values,
        feasible_mask,
    })
}

fn varying_paths(archive: &StudyArchive) -> Option<Vec<String>> {
    let first = archive.candidates.first()?;
    if archive
        .candidates
        .iter()
        .any(|candidate| candidate.parameters.keys().ne(first.parameters.keys()))
    {
        return None;
    }
    let paths = first
        .parameters
        .keys()
        .filter(|path| {
            archive
                .candidates
                .iter()
                .filter_map(|candidate| candidate.parameters.get(*path))
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        })
        .cloned()
        .collect::<Vec<_>>();
    (paths.len() == 2).then_some(paths)
}

fn parsed_axis(archive: &StudyArchive, path: &str) -> Option<Vec<(f64, String)>> {
    archive
        .candidates
        .iter()
        .map(|candidate| {
            candidate
                .parameters
                .get(path)
                .and_then(|raw| parse_value(raw))
        })
        .collect()
}

fn parse_value(raw: &str) -> Option<(f64, String)> {
    let mut parts = raw.splitn(2, char::is_whitespace);
    let value = parts.next()?.parse::<f64>().ok()?;
    value
        .is_finite()
        .then(|| (value, parts.next().map_or("1", str::trim).to_owned()))
}

fn row_major_values(
    archive: &StudyArchive,
    outcomes: &BTreeMap<&str, &CandidateOutcome>,
    paths: &[String],
    x_values: &[f64],
    y_values: &[f64],
    objective_id: &str,
) -> Option<(Vec<f64>, Vec<bool>)> {
    let size = x_values.len().checked_mul(y_values.len())?;
    let mut values = vec![f64::NAN; size];
    let mut feasible = vec![false; size];
    let mut populated = vec![false; size];
    for candidate in &archive.candidates {
        let x = parse_value(candidate.parameters.get(&paths[0])?)?.0;
        let y = parse_value(candidate.parameters.get(&paths[1])?)?.0;
        let column = value_index(x_values, x)?;
        let row = value_index(y_values, y)?;
        let index = row * x_values.len() + column;
        if populated[index] {
            return None;
        }
        let outcome = outcomes.get(candidate.candidate_id.as_str())?;
        values[index] = *outcome.objective_values.get(objective_id)?;
        feasible[index] = outcome.feasible;
        populated[index] = true;
    }
    populated
        .iter()
        .all(|value| *value)
        .then_some((values, feasible))
}

fn unique_values(parsed: &[(f64, String)]) -> Vec<f64> {
    let mut values = parsed.iter().map(|(value, _)| *value).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| left.to_bits() == right.to_bits());
    values
}

fn common_unit(parsed: &[(f64, String)]) -> Option<String> {
    let unit = parsed.first()?.1.clone();
    parsed
        .iter()
        .all(|(_, candidate)| candidate == &unit)
        .then_some(unit)
}

fn value_index(values: &[f64], target: f64) -> Option<usize> {
    values
        .iter()
        .position(|value| value.to_bits() == target.to_bits())
}

#[cfg(test)]
mod tests;
