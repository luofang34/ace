use super::*;
use std::collections::BTreeSet;

pub(super) fn validate(plan: &StudyPlan, model: &VehicleModel) -> StudyResult<()> {
    model.validate()?;
    if plan.schema_version != 1 || plan.max_evaluations == 0 {
        return Err(StudyError::invalid(
            "plan",
            "schema_version must be 1 and budget must be positive",
        ));
    }
    let names = match &plan.method {
        SearchMethod::Grid { axes } => axes.keys().cloned().collect::<Vec<_>>(),
        SearchMethod::Refine { levels, variables } => {
            if !(1..=52).contains(levels) || model.objectives.is_empty() {
                return Err(StudyError::invalid(
                    "refine",
                    "requires objectives and 1 through 52 levels",
                ));
            }
            variables.clone()
        }
    };
    let selected = variables(model, &names)?;
    if selected.is_empty() || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
        return Err(StudyError::invalid(
            "variables",
            "provide distinct declared design variables",
        ));
    }
    for variable in &selected {
        if model.parameter(&variable.name)?.expression.is_some()
            || model.analyses.values().any(|analysis| {
                analysis
                    .outputs
                    .values()
                    .any(|output| output == &variable.name)
            })
        {
            return Err(StudyError::invalid(
                &variable.name,
                "computed outputs cannot be studied as inputs",
            ));
        }
    }
    if let SearchMethod::Grid { axes } = &plan.method {
        if axes.is_empty() {
            return Err(StudyError::invalid("axes", "provide at least one axis"));
        }
        for variable in selected {
            let values = axes
                .get(&variable.name)
                .ok_or_else(|| StudyError::invalid(&variable.name, "missing axis"))?
                .samples(&variable.name)?;
            if values.is_empty() {
                return Err(StudyError::invalid(
                    &variable.name,
                    "axis must contain samples",
                ));
            }
            for value in values {
                sample(model, &variable, value)?;
            }
        }
    }
    Ok(())
}
