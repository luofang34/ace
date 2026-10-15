use super::*;
use crate::{StudyError, plan::Variable};

pub(super) fn run(search: &mut Search, levels: u32, names: &[String]) -> StudyResult<StopReason> {
    let variables = crate::plan::variables(search.baseline.model(), names)?;
    let seed = variables
        .iter()
        .map(|variable| {
            let parameter = search.baseline.model().parameter(&variable.name)?;
            let midpoint = variable.lower.value * 0.5 + variable.upper.value * 0.5;
            let value = parameter
                .value
                .map(|value| value.to(parameter.unit))
                .transpose()?
                .map_or(midpoint, |q| q.value);
            Ok((
                variable.name.clone(),
                Quantity::new(
                    value.clamp(variable.lower.value, variable.upper.value),
                    parameter.unit,
                )?,
            ))
        })
        .collect::<StudyResult<_>>()?;
    if !search.consider(seed)? {
        return Ok(StopReason::EvaluationBudget);
    }
    for level in 0..levels {
        let ids = crate::ranking::incumbents(&search.archive.candidates)?;
        let centers: Vec<_> = search
            .archive
            .candidates
            .iter()
            .filter(|candidate| ids.contains(&candidate.id))
            .map(|candidate| candidate.updates.clone())
            .collect();
        for center in centers {
            for variable in &variables {
                for sign in [-1.0, 1.0] {
                    let updates = neighbor(&center, variable, level, sign)?;
                    if !search.consider(updates)? {
                        return Ok(StopReason::EvaluationBudget);
                    }
                }
            }
        }
    }
    Ok(StopReason::RefinementLimit)
}

fn neighbor(
    center: &BTreeMap<String, Quantity>,
    variable: &Variable,
    level: u32,
    sign: f64,
) -> StudyResult<BTreeMap<String, Quantity>> {
    let origin = center.get(&variable.name).ok_or_else(|| {
        StudyError::invalid(&variable.name, "search center is missing its variable")
    })?;
    let step =
        (variable.upper.value * 0.5 - variable.lower.value * 0.5) * 2.0_f64.powf(-f64::from(level));
    let value = (origin.value + sign * step).clamp(variable.lower.value, variable.upper.value);
    let mut updates = center.clone();
    updates.insert(
        variable.name.clone(),
        Quantity::new(if value == 0.0 { 0.0 } else { value }, origin.unit)?,
    );
    Ok(updates)
}
