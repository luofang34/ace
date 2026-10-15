use crate::{Candidate, Feasibility, StudyResult};
use ace_model::{Direction, Priority};

pub(crate) fn front(candidates: &[Candidate], quality: Feasibility) -> StudyResult<Vec<String>> {
    let eligible: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.feasibility == quality && !candidate.objectives.is_empty())
        .collect();
    nondominated(&eligible, dominates)
}

fn nondominated(
    candidates: &[&Candidate],
    dominates: impl Fn(&Candidate, &Candidate) -> StudyResult<bool>,
) -> StudyResult<Vec<String>> {
    let mut frontier = Vec::new();
    for candidate in candidates {
        let mut dominated = false;
        for other in candidates {
            if candidate.id != other.id && dominates(other, candidate)? {
                dominated = true;
                break;
            }
        }
        if !dominated {
            frontier.push(candidate.id.clone());
        }
    }
    frontier.sort();
    Ok(frontier)
}

fn dominates(left: &Candidate, right: &Candidate) -> StudyResult<bool> {
    let mut better = false;
    for (id, objective) in &left.objectives {
        let Some(other) = right.objectives.get(id) else {
            return Ok(false);
        };
        let (Some(a), Some(b)) = (objective.value.quantity, other.value.quantity) else {
            return Ok(false);
        };
        let a = a.to(b.unit)?.value;
        let (a, b) = match objective.direction {
            Direction::Min => (a, b.value),
            Direction::Max => (-a, -b.value),
        };
        if a > b {
            return Ok(false);
        }
        better |= a < b;
    }
    Ok(better)
}

pub(crate) fn incumbents(candidates: &[Candidate]) -> StudyResult<Vec<String>> {
    let mut chosen = front(candidates, Feasibility::Satisfied)?;
    chosen.extend(front(candidates, Feasibility::Provisional)?);
    if !chosen.is_empty() {
        return Ok(chosen);
    }
    let candidates = candidates.iter().collect::<Vec<_>>();
    nondominated(&candidates, restores)
}

fn restores(left: &Candidate, right: &Candidate) -> StudyResult<bool> {
    let mut better = false;
    for (id, requirement) in &left.model.requirements {
        if requirement.priority != Priority::Hard {
            continue;
        }
        let pair = left
            .margins
            .get(id)
            .and_then(|a| a.absolute)
            .zip(right.margins.get(id).and_then(|b| b.absolute));
        let Some((a, b)) = pair else {
            return Ok(false);
        };
        let a = a.to(b.unit)?.value.min(0.0);
        let b = b.value.min(0.0);
        if a < b {
            return Ok(false);
        }
        better |= a > b;
    }
    Ok(better)
}
