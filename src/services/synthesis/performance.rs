use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec,
    SolverError,
};
use ace_model::{DomainBound, DomainLimit, DomainVariable, Quantity, Unit, ValidityDomain};
use std::collections::BTreeMap;

use crate::models::performance::PointAnalyzer;
use crate::services::model_preflight::preflight_model_domains;

mod constraint_diagram;
mod inputs;

pub(super) use constraint_diagram::ConstraintDiagram;

const ID: &str = "aircraft.performance_envelope";

/// Speed, ceiling and field-length envelope from the native point-performance models.
pub(super) struct PerformanceEnvelope {
    spec: PluginSpec,
}

const INPUTS: [(&str, Unit); 16] = [
    ("area", Unit::SquareMeter),
    ("span", Unit::Meter),
    ("sweep", Unit::Degree),
    ("maximum_takeoff_mass", Unit::Kilogram),
    ("operating_empty_mass", Unit::Kilogram),
    ("maximum_payload_mass", Unit::Kilogram),
    ("maximum_fuel_mass", Unit::Kilogram),
    ("takeoff_mass", Unit::Kilogram),
    ("landing_mass", Unit::Kilogram),
    ("engine_count", Unit::Ratio),
    ("sizing_factor", Unit::Ratio),
    ("field_elevation", Unit::Meter),
    ("obstacle_height", Unit::Meter),
    ("braking_coefficient", Unit::Ratio),
    ("runway_slope", Unit::Ratio),
    ("headwind", Unit::MeterPerSecond),
];

const OUTPUTS: [(&str, Unit); 10] = [
    ("stall_speed_clean", Unit::MeterPerSecond),
    ("stall_speed_landing", Unit::MeterPerSecond),
    ("best_glide_speed", Unit::MeterPerSecond),
    ("minimum_power_speed", Unit::MeterPerSecond),
    ("maximum_level_speed", Unit::MeterPerSecond),
    ("maximum_lift_to_drag", Unit::Ratio),
    ("service_ceiling", Unit::Meter),
    ("absolute_ceiling", Unit::Meter),
    ("takeoff_field_length", Unit::Meter),
    ("landing_field_length", Unit::Meter),
];

impl PerformanceEnvelope {
    pub fn new() -> Self {
        Self {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain: domain(),
                identity: PluginIdentity {
                    id: ID.into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: ace_engine::implementation_identity().digest,
                },
                inputs: INPUTS
                    .iter()
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

impl AnalysisPlugin for PerformanceEnvelope {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let failed = |source: crate::domain::diagnostic::AexError| SolverError::External {
            solver: ID.into(),
            source: Box::new(source),
        };
        let scenario = inputs::scenario(ID, inputs, &context.resources).map_err(failed)?;
        preflight_model_domains(&scenario).map_err(failed)?;
        let summary = PointAnalyzer::new(scenario).summary(None).map_err(failed)?;
        let required = |value: Option<f64>, name: &str| {
            value.ok_or_else(|| SolverError::Failed(format!("{ID} found no {name}")))
        };
        let values = [
            summary.stall_speed_clean_m_s,
            summary.stall_speed_landing_m_s,
            summary.best_glide_speed_m_s,
            summary.minimum_power_speed_m_s,
            summary.maximum_level_speed_m_s,
            summary.maximum_lift_to_drag_ratio,
            required(summary.service_ceiling_m, "service ceiling")?,
            required(summary.absolute_ceiling_m, "absolute ceiling")?,
            required(summary.takeoff_field_length_m, "takeoff field length")?,
            required(summary.landing_field_length_m, "landing field length")?,
        ];
        let values = OUTPUTS
            .iter()
            .zip(values)
            .map(|((name, unit), value)| Ok(((*name).to_owned(), Quantity::new(value, *unit)?)))
            .collect::<Result<BTreeMap<_, _>, SolverError>>()?;
        let approximate = summary
            .metric_validity
            .values()
            .any(|validity| validity.status != crate::domain::validity::ValidityStatus::Valid);
        Ok(PluginOutput {
            values,
            approximate,
            artifacts: Vec::new(),
            details: serde_json::json!({
                "metric_validity": summary.metric_validity,
                "ceiling_limited_by": summary.ceiling_limited_by,
                "physical_service_ceiling_m": summary.physical_service_ceiling_m,
                "physical_absolute_ceiling_m": summary.physical_absolute_ceiling_m,
                "model": summary.model,
                "warnings": summary.warnings,
            }),
            failure: None,
        })
    }
}

fn domain() -> ValidityDomain {
    let at_least_zero = DomainLimit {
        value: 0.0,
        inclusive: true,
    };
    let bounds = INPUTS
        .iter()
        .filter(|(name, _)| !matches!(*name, "sweep" | "runway_slope" | "headwind"))
        .map(|(name, unit)| {
            let mut bound = DomainBound::positive(DomainVariable::Input((*name).into()), *unit);
            match *name {
                "field_elevation"
                | "obstacle_height"
                | "maximum_payload_mass"
                | "maximum_fuel_mass" => bound.lower = Some(at_least_zero),
                "braking_coefficient" => {
                    bound.upper = Some(DomainLimit {
                        value: 1.0,
                        inclusive: true,
                    });
                }
                "sizing_factor" => {
                    bound.lower = Some(DomainLimit {
                        value: 0.5,
                        inclusive: true,
                    });
                    bound.upper = Some(DomainLimit {
                        value: 2.0,
                        inclusive: true,
                    });
                }
                _ => {}
            }
            bound
        })
        .collect();
    ValidityDomain {
        bounds,
        assumptions: vec![
            "ISA standard day; quasi-steady point performance from parabolic or tabulated polars and the selected engine and propeller profiles".into(),
            "Registered aerodynamic, propulsion and atmosphere model domains are enforced; published ceilings respect declared airframe and engine limits".into(),
        ],
    }
}

#[cfg(test)]
mod tests;
