use crate::{StudyError, StudyResult};
use ace_engine::Session;
use ace_model::{ModelEdit, Quantity, Source};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One data-only configuration choice; structural edits precede scalar updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantOption {
    /// Atomic structural transaction applied to an independent session.
    #[serde(default)]
    pub edits: Vec<ModelEdit>,
    /// Independent parameter overrides with explicit units.
    #[serde(default)]
    pub updates: BTreeMap<String, Quantity>,
}

impl VariantOption {
    /// Build a fresh session; a rejected choice never mutates the baseline or reuses old runs.
    pub fn instantiate(&self, baseline: &Session) -> StudyResult<Session> {
        let mut session = baseline.branch()?;
        session.transact(&self.edits)?;
        session.edit(
            &self.updates,
            Source {
                kind: "variant".into(),
                reference: crate::record::digest(self)?,
                revision: None,
            },
        )?;
        Ok(session)
    }
}

/// Bounded exhaustive matrix for small, explicitly supported configuration families.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantMatrix {
    /// Named axes, each containing stable choice identifiers and typed edits.
    pub axes: BTreeMap<String, BTreeMap<String, VariantOption>>,
    /// Forbidden partial assignments; every matching complete assignment is excluded.
    #[serde(default)]
    pub exclusions: Vec<BTreeMap<String, String>>,
    /// Maximum Cartesian size before filtering; prevents unbounded expansion.
    pub max_combinations: usize,
}

/// One reproducible assignment and its combined configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantCase {
    /// Content identity covering the selected choices and combined edits.
    pub id: String,
    /// Choice identifier for every axis.
    pub choices: BTreeMap<String, String>,
    /// Combined configuration; conflicting writes are rejected during expansion.
    pub configuration: VariantOption,
}

impl VariantMatrix {
    /// Expand in canonical axis/choice order, retaining all legal combinations.
    /// This is exhaustive coverage of the bounded matrix, not a pairwise sampler.
    pub fn expand(&self) -> StudyResult<Vec<VariantCase>> {
        self.validate()?;
        let mut assignments = vec![BTreeMap::new()];
        for (axis, choices) in &self.axes {
            assignments = assignments
                .into_iter()
                .flat_map(|assignment| {
                    choices.keys().map(move |choice| {
                        let mut selected = assignment.clone();
                        selected.insert(axis.clone(), choice.clone());
                        selected
                    })
                })
                .collect();
        }
        assignments
            .into_iter()
            .filter(|assignment| {
                !self.exclusions.iter().any(|excluded| {
                    excluded
                        .iter()
                        .all(|(axis, choice)| assignment.get(axis) == Some(choice))
                })
            })
            .map(|choices| self.case(choices))
            .collect()
    }

    fn validate(&self) -> StudyResult<()> {
        if self.max_combinations == 0 || self.max_combinations > 10_000 {
            return Err(StudyError::invalid(
                "max_combinations",
                "must be in 1..=10000",
            ));
        }
        let mut count = 1_usize;
        for (axis, choices) in &self.axes {
            if axis.trim().is_empty()
                || choices.is_empty()
                || choices.keys().any(|name| name.trim().is_empty())
            {
                return Err(StudyError::invalid(
                    axis,
                    "axis and choices must be nonempty",
                ));
            }
            count = count
                .checked_mul(choices.len())
                .ok_or_else(|| StudyError::invalid(axis, "matrix size overflow"))?;
            if count > self.max_combinations {
                return Err(StudyError::invalid(
                    axis,
                    "Cartesian size exceeds max_combinations",
                ));
            }
        }
        for excluded in &self.exclusions {
            if excluded.is_empty()
                || excluded.iter().any(|(axis, choice)| {
                    self.axes
                        .get(axis)
                        .is_none_or(|choices| !choices.contains_key(choice))
                })
            {
                return Err(StudyError::invalid(
                    "exclusions",
                    "empty or unknown assignment",
                ));
            }
        }
        Ok(())
    }

    fn case(&self, choices: BTreeMap<String, String>) -> StudyResult<VariantCase> {
        let mut configuration = VariantOption::default();
        let mut written = BTreeSet::new();
        for (axis, name) in &choices {
            let option = self
                .axes
                .get(axis)
                .and_then(|axis| axis.get(name))
                .ok_or_else(|| StudyError::invalid(axis, "missing choice"))?;
            let targets = option
                .edits
                .iter()
                .map(edit_target)
                .chain(option.updates.keys().map(|id| format!("parameter:{id}")));
            for target in targets {
                if !written.insert(target.clone()) {
                    return Err(StudyError::invalid(
                        axis,
                        format!("ambiguous writes to {target}"),
                    ));
                }
            }
            configuration.edits.extend(option.edits.clone());
            configuration.updates.extend(option.updates.clone());
        }
        let id = crate::record::digest(&(&choices, &configuration))?;
        Ok(VariantCase {
            id,
            choices,
            configuration,
        })
    }
}

fn edit_target(edit: &ModelEdit) -> String {
    match edit {
        ModelEdit::Parameter { id, .. } => format!("parameter:{id}"),
        ModelEdit::Geometry { id, .. } => format!("geometry:{id}"),
        ModelEdit::MassItem { id, .. } => format!("mass_item:{id}"),
        ModelEdit::Mission { id, .. } => format!("mission:{id}"),
        ModelEdit::Requirement { id, .. } => format!("requirement:{id}"),
        ModelEdit::Analysis { id, .. } => format!("analysis:{id}"),
        ModelEdit::Closure { id, .. } => format!("closure:{id}"),
        ModelEdit::Clearance { id, .. } => format!("clearance:{id}"),
        ModelEdit::Section { id, .. } => format!("section:{id}"),
        ModelEdit::LoadingCase { id, .. } => format!("loading:{id}"),
        ModelEdit::Objective { id, .. } => format!("objective:{id}"),
        ModelEdit::Operations { .. } => "operations".into(),
        ModelEdit::DesignVariables { .. } => "design_variables".into(),
    }
}

#[cfg(test)]
mod tests;
