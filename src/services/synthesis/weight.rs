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
                mode: ace_model::AnalysisMode::Scalar,
                domain: domain(),
                identity: PluginIdentity {
                    id: "aircraft.weight_closure".into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: ace_engine::implementation_identity().digest,
                },
                inputs: BTreeMap::from([
                    ("payload".into(), Unit::Kilogram.into()),
                    ("fuel".into(), Unit::Kilogram.into()),
                    ("empty_coefficient".into(), Unit::Ratio.into()),
                    ("empty_exponent".into(), Unit::Ratio.into()),
                    ("lower_mass".into(), Unit::Kilogram.into()),
                    ("upper_mass".into(), Unit::Kilogram.into()),
                ]),
                outputs: BTreeMap::from([
                    ("mtow".into(), Unit::Kilogram.into()),
                    ("residual".into(), Unit::Kilogram.into()),
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

fn domain() -> ace_model::ValidityDomain {
    use ace_model::{DomainBound, DomainLimit, DomainVariable, ValidityDomain};
    let bounds = [
        ("payload", Unit::Kilogram),
        ("fuel", Unit::Kilogram),
        ("empty_coefficient", Unit::Ratio),
        ("empty_exponent", Unit::Ratio),
        ("lower_mass", Unit::Kilogram),
        ("upper_mass", Unit::Kilogram),
    ]
    .into_iter()
    .map(|(name, unit)| {
        let mut bound = DomainBound::positive(DomainVariable::Input(name.into()), unit);
        if matches!(name, "payload" | "fuel") {
            bound.lower = Some(DomainLimit {
                value: 0.0,
                inclusive: true,
            });
        }
        bound
    })
    .collect();
    ValidityDomain { bounds, assumptions: vec![
        "Empirical empty-mass power law; the solver also requires an ordered sign-changing mass bracket".into(),
    ] }
}
