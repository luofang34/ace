use crate::DomainPanel;
use ace_engine::{Margin, MarginStatus, Snapshot, ValueStatus};
use ace_model::{Requirement, VehicleModel};
use serde::Serialize;
use std::collections::BTreeMap;

/// Current domain evidence to bring into view when inspecting a requirement.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConstraintFocus {
    /// Loading that produces the constrained CG limit, if established by the solver.
    pub loading: Option<LoadingFocus>,
    /// Relevant sections in increasing envelope cross-sectional area.
    pub sections: Vec<SectionFocus>,
}

/// A solver-retained loading witness, including its exact calculation.
#[derive(Debug, Clone, Serialize)]
pub struct LoadingFocus {
    /// Mass analysis identifier.
    pub analysis: String,
    /// Named loading, or `None` for the model's current loading.
    pub case: Option<String>,
    /// Content address of the producing calculation.
    pub run_id: String,
}

/// A geometric section associated with a requirement's semantic objects.
#[derive(Debug, Clone, Serialize)]
pub struct SectionFocus {
    /// Geometry analysis identifier.
    pub analysis: String,
    /// Identifier in the analysis's structured section results.
    pub section: String,
    /// Content address of the producing calculation.
    pub run_id: String,
}

pub(crate) fn build(
    model: &VehicleModel,
    snapshot: &Snapshot,
    margins: &BTreeMap<String, Margin>,
    panels: &[DomainPanel],
) -> BTreeMap<String, ConstraintFocus> {
    model
        .requirements
        .iter()
        .filter_map(|(id, requirement)| {
            let margin = margins.get(id)?;
            if matches!(
                margin.status,
                MarginStatus::Unknown | MarginStatus::NotApplicable
            ) {
                return None;
            }
            let focus = ConstraintFocus {
                loading: loading(model, snapshot, requirement),
                sections: sections(requirement, panels),
            };
            (focus.loading.is_some() || !focus.sections.is_empty()).then(|| (id.clone(), focus))
        })
        .collect()
}

fn loading(
    model: &VehicleModel,
    snapshot: &Snapshot,
    requirement: &Requirement,
) -> Option<LoadingFocus> {
    let value = snapshot.values.get(&requirement.metric)?;
    let run = snapshot
        .runs
        .iter()
        .find(|run| Some(&run.id) == value.run_id.as_ref())?;
    if run.plugin.as_ref()?.id != "vehicle.mass_properties" {
        return None;
    }
    let id = run.node.strip_prefix("analysis:")?;
    let analysis = model.analyses.get(id)?;
    let (port, _) = analysis
        .outputs
        .iter()
        .find(|(_, name)| *name == &requirement.metric)?;
    let key = match port.as_str() {
        "minimum_cg_x" => "minimum_cg_x_loading",
        "maximum_cg_x" => "maximum_cg_x_loading",
        _ => return None,
    };
    let witness = run.details.get(key)?;
    let case = if witness.is_null() {
        None
    } else {
        let name = witness.as_str()?;
        run.resources.loading_cases.get(name)?;
        Some(name.into())
    };
    Some(LoadingFocus {
        analysis: id.into(),
        case,
        run_id: run.id.clone(),
    })
}

fn sections(requirement: &Requirement, panels: &[DomainPanel]) -> Vec<SectionFocus> {
    let mut results = Vec::new();
    for panel in panels.iter().filter(|panel| {
        panel.plugin == "vehicle.geometry_envelopes"
            && !panel.historical
            && matches!(
                panel.status,
                ValueStatus::Computed | ValueStatus::Approximate
            )
    }) {
        for section in panel
            .details
            .get("sections")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let Some(id) = section.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let area = section
                .get("slices")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|slice| section_area(slice, &requirement.geometry))
                .reduce(f64::min);
            if let Some(area) = area {
                results.push((
                    area,
                    SectionFocus {
                        analysis: panel.analysis.clone(),
                        section: id.into(),
                        run_id: panel.run_id.clone(),
                    },
                ));
            }
        }
    }
    results.sort_by(|(a, left), (b, right)| {
        a.total_cmp(b)
            .then(left.analysis.cmp(&right.analysis))
            .then(left.section.cmp(&right.section))
    });
    results.into_iter().map(|(_, section)| section).collect()
}

fn section_area(slice: &serde_json::Value, geometry: &[String]) -> Option<f64> {
    let id = slice.get("geometry")?.as_str()?;
    if !geometry.iter().any(|item| item == id) {
        return None;
    }
    let low = slice.get("minimum_m")?.as_array()?;
    let high = slice.get("maximum_m")?.as_array()?;
    let width = high.first()?.as_f64()? - low.first()?.as_f64()?;
    let height = high.get(1)?.as_f64()? - low.get(1)?.as_f64()?;
    let area = width * height;
    (width >= 0.0 && height >= 0.0 && area.is_finite()).then_some(area)
}
