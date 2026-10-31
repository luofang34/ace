use super::*;
use crate::StudyError;

pub(super) fn run(
    search: &mut Search,
    requested: &BTreeMap<String, crate::Axis>,
) -> StudyResult<StopReason> {
    let variables = crate::plan::variables(
        search.baseline.model(),
        &requested.keys().cloned().collect::<Vec<_>>(),
    )?;
    let mut axes = Vec::new();
    for variable in variables {
        let values = requested
            .get(&variable.name)
            .ok_or_else(|| StudyError::invalid(&variable.name, "missing axis"))?
            .samples(&variable.name)?;
        let mut normalized = Vec::new();
        for value in values {
            let mut value = crate::plan::sample(search.baseline.model(), &variable, value)?;
            if value.value == 0.0 {
                value.value = 0.0;
            }
            if !normalized
                .iter()
                .any(|existing: &Quantity| existing.value == value.value)
            {
                normalized.push(value);
            }
        }
        axes.push((variable.name, normalized));
    }
    let mut indices = vec![0; axes.len()];
    loop {
        let updates = axes
            .iter()
            .zip(&indices)
            .map(|((name, values), index)| {
                values
                    .get(*index)
                    .copied()
                    .map(|value| (name.clone(), value))
                    .ok_or_else(|| StudyError::invalid(name, "grid sample index exceeds axis"))
            })
            .collect::<StudyResult<_>>()?;
        if !search.consider(updates)? {
            return Ok(StopReason::EvaluationBudget);
        }
        if !advance(&mut indices, &axes) {
            return Ok(StopReason::GridExhausted);
        }
    }
}

fn advance(indices: &mut [usize], axes: &[(String, Vec<Quantity>)]) -> bool {
    for (index, (_, values)) in indices.iter_mut().zip(axes).rev() {
        *index = index.wrapping_add(1);
        if *index < values.len() {
            return true;
        }
        *index = 0;
    }
    false
}
