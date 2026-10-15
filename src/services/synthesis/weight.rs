use crate::models::weight::{WeightClosureInput, solve_weight_closure};
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec,
    SolverError,
};
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

pub(super) struct WeightPlugin {
    spec: PluginSpec,
}

impl WeightPlugin {
    pub fn new() -> Self {
        Self {
            spec: PluginSpec {
                identity: PluginIdentity {
                    id: "aircraft.weight_closure".into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: ace_engine::implementation_identity().digest,
                },
                inputs: BTreeMap::from([
                    ("payload".into(), Unit::Kilogram),
                    ("fuel".into(), Unit::Kilogram),
                    ("empty_coefficient".into(), Unit::Ratio),
                    ("empty_exponent".into(), Unit::Ratio),
                    ("lower_mass".into(), Unit::Kilogram),
                    ("upper_mass".into(), Unit::Kilogram),
                ]),
                outputs: BTreeMap::from([
                    ("mtow".into(), Unit::Kilogram),
                    ("residual".into(), Unit::Kilogram),
                ]),
            },
        }
    }
}

impl AnalysisPlugin for WeightPlugin {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let input = WeightClosureInput {
            payload_kg: value(inputs, "payload")?,
            fuel_kg: value(inputs, "fuel")?,
            empty_coefficient: value(inputs, "empty_coefficient")?,
            empty_exponent: value(inputs, "empty_exponent")?,
            lower_mass_kg: value(inputs, "lower_mass")?,
            upper_mass_kg: value(inputs, "upper_mass")?,
        };
        if input.payload_kg < 0.0
            || input.fuel_kg < 0.0
            || input.empty_coefficient <= 0.0
            || input.empty_exponent <= 0.0
            || input.lower_mass_kg <= 0.0
            || input.upper_mass_kg <= input.lower_mass_kg
        {
            return Err(SolverError::Failed(
                "invalid weight-closure input or mass bracket".into(),
            ));
        }
        let result = solve_weight_closure(input).map_err(|source| SolverError::External {
            solver: self.spec.identity.id.clone(),
            source: Box::new(source),
        })?;
        Ok(PluginOutput {
            values: BTreeMap::from([
                (
                    "mtow".into(),
                    Quantity::new(result.takeoff_mass_kg, Unit::Kilogram)?,
                ),
                (
                    "residual".into(),
                    Quantity::new(result.residual_kg, Unit::Kilogram)?,
                ),
            ]),
            approximate: false,
            artifacts: Vec::new(),
            details: serde_json::Value::Null,
            failure: None,
        })
    }
}

fn value(inputs: &BTreeMap<String, Quantity>, port: &str) -> Result<f64, SolverError> {
    inputs
        .get(port)
        .map(|quantity| quantity.value)
        .ok_or_else(|| SolverError::Failed(format!("missing weight input port {port}")))
}
