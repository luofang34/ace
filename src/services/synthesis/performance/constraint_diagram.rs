//! Wing-loading constraint diagram over the vehicle's catalog polars and engine.

use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec,
    SolverError,
};
use ace_model::{DomainBound, DomainVariable, Quantity, Unit};
use std::collections::BTreeMap;

use super::{INPUTS, domain, inputs};
use crate::domain::diagnostic::AexError;
use crate::models::constraints::{ConstraintAnalyzer, ConstraintTargets};
use crate::services::model_preflight::preflight_model_domains;

const ID: &str = "aircraft.constraint_diagram";

/// Wing-loading samples across the declared range, matching the scenario command default.
const SAMPLES: u32 = 80;

/// Mission and requirement values the curves are drawn against, plus the plotted range.
const TARGETS: [(&str, Unit); 5] = [
    ("design_speed", Unit::MeterPerSecond),
    ("landing_stall_speed_limit", Unit::MeterPerSecond),
    ("takeoff_distance_limit", Unit::Meter),
    ("wing_loading_minimum", Unit::Pascal),
    ("wing_loading_maximum", Unit::Pascal),
];

/// Ratios at or above one mean the selected design satisfies every curve and the stall limit.
const OUTPUTS: [(&str, Unit); 4] = [
    ("selected_wing_loading", Unit::Pascal),
    ("stall_wing_loading_limit", Unit::Pascal),
    ("installed_to_required_loading", Unit::Ratio),
    ("stall_limit_to_selected_wing_loading", Unit::Ratio),
];

/// Cruise, climb and approximate takeoff loading curves against wing loading.
pub(in crate::services::synthesis) struct ConstraintDiagram {
    spec: PluginSpec,
}

impl ConstraintDiagram {
    pub fn new() -> Self {
        let mut domain = domain();
        domain.bounds.extend(TARGETS.iter().map(|(name, unit)| {
            DomainBound::positive(DomainVariable::Input((*name).into()), *unit)
        }));
        Self {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain,
                identity: PluginIdentity {
                    id: ID.into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: ace_engine::implementation_identity().digest,
                },
                inputs: INPUTS
                    .iter()
                    .chain(TARGETS.iter())
                    .map(|(name, unit)| ((*name).into(), (*unit).into()))
                    .collect(),
                outputs: OUTPUTS
                    .iter()
                    .map(|(name, unit)| ((*name).into(), (*unit).into()))
                    .collect(),
            },
        }
    }
}

impl AnalysisPlugin for ConstraintDiagram {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let failed = |source: AexError| SolverError::External {
            solver: ID.into(),
            source: Box::new(source),
        };
        let port = |name: &str| {
            inputs
                .get(name)
                .map(|quantity| quantity.value)
                .ok_or_else(|| SolverError::Failed(format!("{ID} is missing input {name}")))
        };
        let (minimum, maximum) = (port("wing_loading_minimum")?, port("wing_loading_maximum")?);
        if minimum >= maximum {
            return Err(SolverError::Failed(
                "wing_loading_minimum must be below wing_loading_maximum".into(),
            ));
        }
        let targets = ConstraintTargets {
            design_speed_m_s: port("design_speed")?,
            landing_stall_speed_m_s: port("landing_stall_speed_limit")?,
            takeoff_distance_m: port("takeoff_distance_limit")?,
        };
        let scenario = inputs::scenario(ID, inputs, &context.resources).map_err(failed)?;
        preflight_model_domains(&scenario).map_err(failed)?;
        let analyzer = ConstraintAnalyzer::new(scenario);
        let diagram = analyzer
            .analyze_with(targets, minimum, maximum, SAMPLES)
            .map_err(failed)?;
        let selected = diagram.selected_wing_loading_n_m2;
        // The sampled grid rarely contains the design point, so its requirement is evaluated there.
        let at_selection = analyzer
            .analyze_with(targets, selected, selected, 2)
            .map_err(failed)?;
        let required = at_selection
            .constraints
            .values()
            .filter_map(|curve| curve.first())
            .copied()
            .fold(0.0, f64::max);
        if required <= 0.0 {
            return Err(SolverError::Failed(format!(
                "{ID} found no positive loading requirement at the selected wing loading"
            )));
        }
        let values = [
            selected,
            diagram.stall_wing_loading_limit_n_m2,
            diagram.selected_loading / required,
            diagram.stall_wing_loading_limit_n_m2 / selected,
        ];
        let values = OUTPUTS
            .iter()
            .zip(values)
            .map(|((name, unit), value)| Ok(((*name).to_owned(), Quantity::new(value, *unit)?)))
            .collect::<Result<BTreeMap<_, _>, SolverError>>()?;
        Ok(PluginOutput {
            values,
            approximate: true,
            artifacts: Vec::new(),
            details: serde_json::json!({
                "required_loading_at_selection": required,
                "diagram": diagram,
            }),
            failure: None,
        })
    }
}
