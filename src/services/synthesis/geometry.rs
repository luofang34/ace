use crate::backends::{openvsp::OpenVspBackend, registry::BackendRegistry};
use crate::domain::diagnostic::AexResult;
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec, Registry,
    SolverError,
};
use ace_model::{Artifact, Quantity, Unit, VehicleModel};
use std::{collections::BTreeMap, sync::Arc};

const ID: &str = "aircraft.openvsp_wing";

pub(super) fn register_blocking(model: &VehicleModel, registry: &mut Registry) -> AexResult<()> {
    if !model
        .analyses
        .values()
        .any(|analysis| analysis.plugin == ID)
    {
        return Ok(());
    }
    let backends = BackendRegistry::detect_blocking();
    match backends.openvsp() {
        Ok(backend) => {
            registry.register(Arc::new(Wing::new_blocking(backend.clone())?))?;
        }
        Err(error) => {
            tracing::warn!(%error, "OpenVSP synthesis adapter unavailable");
        }
    }
    Ok(())
}

struct Wing {
    spec: PluginSpec,
    backend: OpenVspBackend,
    binary: String,
}

impl Wing {
    fn new_blocking(backend: OpenVspBackend) -> AexResult<Self> {
        let binary = backend.binary_identity_blocking()?;
        let identity = PluginIdentity {
            id: ID.into(),
            version: "1".into(),
            fidelity: Fidelity::Conceptual,
            code_revision: format!("{};{binary}", ace_engine::implementation_identity().digest),
        };
        let inputs = [
            ("area", Unit::SquareMeter),
            ("span", Unit::Meter),
            ("taper", Unit::Ratio),
            ("sweep", Unit::Degree),
            ("dihedral", Unit::Degree),
            ("twist", Unit::Degree),
        ]
        .into_iter()
        .map(|(name, unit)| (name.into(), unit.into()))
        .collect();
        Ok(Self {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain: Default::default(),
                identity,
                inputs,
                outputs: BTreeMap::from([("wetted_area".into(), Unit::SquareMeter.into())]),
            },
            backend,
            binary,
        })
    }

    fn evaluate_blocking(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        control: &ace_engine::RunControl,
    ) -> Result<PluginOutput, SolverError> {
        let values: BTreeMap<_, _> = inputs
            .iter()
            .map(|(id, value)| (id.clone(), value.value))
            .collect();
        let read = |name| {
            values
                .get(name)
                .copied()
                .ok_or_else(|| SolverError::Failed(format!("missing wing {name}")))
        };
        let (area, span, taper) = (read("area")?, read("span")?, read("taper")?);
        if area <= 0.0
            || span <= 0.0
            || !(0.0..=1.0).contains(&taper)
            || taper == 0.0
            || read("sweep")?.abs() >= 80.0
            || read("dihedral")?.abs() >= 80.0
            || read("twist")?.abs() >= 45.0
        {
            return Err(SolverError::Failed(
                "canonical wing is outside the adapter's declared domain".into(),
            ));
        }
        if self.backend.binary_identity_blocking().map_err(external)? != self.binary {
            return Err(SolverError::Failed(
                "OpenVSP executable changed; reopen the model to bind its new identity".into(),
            ));
        }
        let geometry = self
            .backend
            .generate_synthesis_wing_blocking(&values, control)
            .map_err(external)?;
        let artifact = Artifact::from_text(
            geometry.content,
            "application/xml",
            "openvsp.vsp3/1",
            self.spec.identity.generator(),
        );
        Ok(PluginOutput {
            values: BTreeMap::from([(
                "wetted_area".into(),
                Quantity::new(geometry.wetted_area, Unit::SquareMeter)?,
            )]),
            approximate: true,
            artifacts: vec![artifact],
            failure: None,
            details: serde_json::json!({"backend":self.binary,"assumptions":"isolated symmetric wing; horizontal span, untwisted projected reference area and leading-edge sweep in local axes; default airfoils; conceptual surface area","inputs":inputs,"readback":geometry.measured}),
        })
    }
}

impl AnalysisPlugin for Wing {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        self.evaluate_blocking(inputs, context.registry.control())
    }
}

fn external(source: crate::domain::diagnostic::AexError) -> SolverError {
    SolverError::External {
        solver: ID.into(),
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;
