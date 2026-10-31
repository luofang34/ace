//! Clean-configuration point performance at one flight condition, optionally CG-trimmed.

use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec,
    SolverError,
};
use ace_model::{DomainBound, DomainLimit, DomainVariable, Quantity, Unit};
use std::collections::BTreeMap;

use super::{INPUTS, domain, inputs};
use crate::domain::diagnostic::AexError;
use crate::domain::validity::ValidityStatus;
use crate::models::performance::{PointAnalyzer, wing_lift_fraction};
use crate::services::model_preflight::preflight_model_domains;

const CONDITION: [(&str, Unit); 3] = [
    ("altitude", Unit::Meter),
    ("true_airspeed", Unit::MeterPerSecond),
    ("mass", Unit::Kilogram),
];

/// Longitudinal stations from one datum; the trimmed variant only.
const STATIONS: [(&str, Unit); 3] = [
    ("center_of_gravity", Unit::Meter),
    ("wing_aerodynamic_center", Unit::Meter),
    ("tail_aerodynamic_center", Unit::Meter),
];

const OUTPUTS: [(&str, Unit); 13] = [
    ("mach", Unit::Ratio),
    ("stall_speed", Unit::MeterPerSecond),
    ("lift_coefficient", Unit::Ratio),
    ("drag_coefficient", Unit::Ratio),
    ("lift_to_drag", Unit::Ratio),
    ("thrust_required", Unit::Newton),
    ("power_required", Unit::Watt),
    ("excess_thrust", Unit::Newton),
    ("excess_power", Unit::Watt),
    ("rate_of_climb", Unit::MeterPerSecond),
    ("climb_gradient", Unit::Ratio),
    ("fuel_flow_required", Unit::KilogramPerSecond),
    ("fuel_flow_maximum_power", Unit::KilogramPerSecond),
];

/// Thrust, power, climb and fuel flow at a declared altitude, airspeed and mass.
pub(in crate::services::synthesis) struct OperatingPoint {
    spec: PluginSpec,
    trimmed: bool,
}

impl OperatingPoint {
    pub fn new() -> Self {
        Self::with_trim(false)
    }

    /// The wing carries the trimmed lift for the CG and stations, which sets stall
    /// speed and induced drag; climb still uses the full mass.
    pub fn trimmed() -> Self {
        Self::with_trim(true)
    }

    fn with_trim(trimmed: bool) -> Self {
        let stations: &[(&str, Unit)] = if trimmed { &STATIONS } else { &[] };
        let mut domain = domain();
        for (name, unit) in CONDITION.iter().chain(stations) {
            let mut bound = DomainBound::positive(DomainVariable::Input((*name).into()), *unit);
            if *name == "altitude" {
                bound.lower = Some(DomainLimit {
                    value: 0.0,
                    inclusive: true,
                });
            }
            domain.bounds.push(bound);
        }
        let mut outputs: BTreeMap<_, _> = OUTPUTS
            .iter()
            .map(|(name, unit)| ((*name).to_owned(), (*unit).into()))
            .collect();
        if trimmed {
            outputs.insert("trim_wing_lift_fraction".into(), Unit::Ratio.into());
        }
        let id = if trimmed {
            "aircraft.trimmed_operating_point"
        } else {
            "aircraft.operating_point"
        };
        Self {
            trimmed,
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain,
                identity: PluginIdentity {
                    id: id.into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: ace_engine::implementation_identity().digest,
                },
                inputs: INPUTS
                    .iter()
                    .chain(CONDITION.iter())
                    .chain(stations)
                    .map(|(name, unit)| ((*name).into(), (*unit).into()))
                    .collect(),
                outputs,
            },
        }
    }
}

impl AnalysisPlugin for OperatingPoint {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let id = self.spec.identity.id.clone();
        let failed = |source: AexError| SolverError::External {
            solver: id.clone(),
            source: Box::new(source),
        };
        let port = |name: &str| {
            inputs
                .get(name)
                .map(|quantity| quantity.value)
                .ok_or_else(|| SolverError::Failed(format!("{id} is missing input {name}")))
        };
        let trim = if self.trimmed {
            let cg = port("center_of_gravity")?;
            let fraction = wing_lift_fraction(
                cg,
                port("wing_aerodynamic_center")?,
                port("tail_aerodynamic_center")?,
            )
            .map_err(failed)?;
            Some((cg, fraction))
        } else {
            None
        };
        let scenario = inputs::scenario(&id, inputs, &context.resources).map_err(failed)?;
        preflight_model_domains(&scenario).map_err(failed)?;
        let point = PointAnalyzer::new(scenario)
            .point_trimmed(
                port("altitude")?,
                port("true_airspeed")?,
                port("mass")?,
                "clean",
                trim,
            )
            .map_err(failed)?;
        let aero = &point.aerodynamics;
        let values = [
            point.mach,
            point.stall_speed_m_s,
            aero.lift_coefficient,
            aero.drag_coefficient,
            aero.lift_coefficient / aero.drag_coefficient,
            point.thrust_required_n,
            point.power_required_w,
            point.excess_thrust_n,
            point.excess_power_w,
            point.rate_of_climb_m_s,
            point.climb_gradient,
            point.fuel_flow_required_kg_s,
            point.fuel_flow_max_power_kg_s,
        ];
        let mut values = OUTPUTS
            .iter()
            .zip(values)
            .map(|((name, unit), value)| Ok(((*name).to_owned(), Quantity::new(value, *unit)?)))
            .collect::<Result<BTreeMap<_, _>, SolverError>>()?;
        if let Some(fraction) = point.trim_wing_lift_fraction {
            values.insert(
                "trim_wing_lift_fraction".into(),
                Quantity::new(fraction, Unit::Ratio)?,
            );
        }
        let approximate = point
            .metric_validity
            .values()
            .any(|validity| validity.status != ValidityStatus::Valid);
        Ok(PluginOutput {
            values,
            approximate,
            artifacts: Vec::new(),
            details: serde_json::json!({ "point": point }),
            failure: None,
        })
    }
}
