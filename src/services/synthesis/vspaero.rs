use crate::backends::{
    openvsp::{OpenVspBackend, polar::PORTS},
    registry::BackendRegistry,
};
use crate::domain::diagnostic::AexResult;
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec, Registry,
    SolverError,
};
use ace_model::{Artifact, Quantity, Unit, VehicleModel};
use std::{collections::BTreeMap, sync::Arc};

const ID: &str = "aircraft.vspaero_wing_polar";

pub(super) fn register_blocking(model: &VehicleModel, registry: &mut Registry) -> AexResult<()> {
    if !model
        .analyses
        .values()
        .any(|analysis| analysis.plugin == ID)
    {
        return Ok(());
    }
    let backends = BackendRegistry::detect_blocking();
    match backends
        .openvsp()
        .and_then(|backend| PolarPlugin::new_blocking(backend.clone()))
    {
        Ok(plugin) => registry.register(Arc::new(plugin))?,
        Err(error) => tracing::warn!(%error, "VSPAERO synthesis adapter unavailable"),
    }
    Ok(())
}

struct PolarPlugin {
    spec: PluginSpec,
    backend: OpenVspBackend,
    binary: String,
}

impl PolarPlugin {
    fn new_blocking(backend: OpenVspBackend) -> AexResult<Self> {
        let binary = backend.polar_identity_blocking()?;
        let identity = PluginIdentity {
            id: ID.into(),
            version: "1".into(),
            fidelity: Fidelity::Surrogate,
            code_revision: format!("{};{binary}", ace_engine::implementation_identity().digest),
        };
        let inputs = [
            ("area", Unit::SquareMeter),
            ("span", Unit::Meter),
            ("taper", Unit::Ratio),
            ("sweep", Unit::Degree),
            ("dihedral", Unit::Degree),
            ("twist", Unit::Degree),
            ("thickness", Unit::Ratio),
        ]
        .into_iter()
        .map(|(name, unit)| (name.into(), unit.into()))
        .collect();
        Ok(Self {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain: domain(),
                identity,
                inputs,
                outputs: PORTS
                    .into_iter()
                    .map(|name| (name.into(), super::flight::contract::polar_port(name)))
                    .collect(),
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
            .map(|(key, value)| (key.clone(), value.value))
            .collect();
        validate(&values)?;
        if self.backend.polar_identity_blocking().map_err(external)? != self.binary {
            return Err(SolverError::Failed(
                "VSPAERO executable changed; reopen to bind its new identity".into(),
            ));
        }
        let polar = self
            .backend
            .synthesis_polar_blocking(&values, control)
            .map_err(external)?;
        let mut output = BTreeMap::new();
        let mut cl_limit = f64::INFINITY;
        for (port, fit) in PORTS.into_iter().zip(&polar.fits) {
            output.insert(port.into(), Quantity::new(fit.k, Unit::Ratio)?);
            cl_limit = cl_limit.min(fit.cl_limit);
        }
        output.insert("cl_limit".into(), Quantity::new(cl_limit, Unit::Ratio)?);
        let evidence = serde_json::to_string(&polar).map_err(|source| SolverError::External {
            solver: ID.into(),
            source: Box::new(source),
        })?;
        let generator = self.spec.identity.generator();
        Ok(PluginOutput {
            values: output,
            approximate: true,
            failure: None,
            artifacts: vec![
                Artifact::from_text(
                    polar.geometry,
                    "application/xml",
                    "openvsp.vsp3/1",
                    generator.clone(),
                ),
                Artifact::from_text(
                    evidence,
                    "application/json",
                    "aircraft.vspaero.polar/1",
                    generator,
                ),
            ],
            details: serde_json::json!({"backend":self.binary,"fits":polar.fits,
                "readback":polar.readback,"mach_interpolation":"linear between samples at Mach 0, 0.2, 0.4, 0.6 and 0.8; no extrapolation",
                "drag":"CD = user parasite CD0 + k(Mach) * CL^2; VSPAERO CDi only, excluding its CDo",
                "fit_tolerance":"absolute CDi 0.00002 + relative 5% at every sampled alpha",
                "validity":"isolated untwisted symmetric NACA 0012 wing; attached flow alpha 0..10 deg; CL <= minimum sampled upper CL across Mach; reference area and span use canonical horizontal projection",
                "limitations":"default mesh/wake settings; no mesh convergence or experimental calibration; no fuselage/tail/interference/trim drag, stall, high-lift devices or transonic shocks"}),
        })
    }
}

fn validate(values: &BTreeMap<String, f64>) -> Result<(), SolverError> {
    let domain = domain();
    let mut inputs = BTreeMap::new();
    for bound in &domain.bounds {
        if let ace_model::DomainVariable::Input(name) = &bound.variable
            && let Some(value) = values.get(name)
        {
            inputs.insert(name.clone(), Quantity::new(*value, bound.unit)?);
        }
    }
    domain.check(&inputs, None)?;
    Ok(())
}

fn domain() -> ace_model::ValidityDomain {
    use ace_model::{DomainBound, DomainVariable, ValidityDomain};
    let input = |name: &str| DomainVariable::Input(name.into());
    let mut bounds = vec![
        DomainBound::positive(input("area"), Unit::SquareMeter),
        DomainBound::positive(input("span"), Unit::Meter),
    ];
    for (name, unit, lower, upper) in [
        ("taper", Unit::Ratio, 0.1, 1.0),
        ("sweep", Unit::Degree, -40.0, 40.0),
        ("dihedral", Unit::Degree, -10.0, 10.0),
        ("twist", Unit::Degree, -1e-10, 1e-10),
        ("thickness", Unit::Ratio, 0.12 - 1e-10, 0.12 + 1e-10),
    ] {
        bounds.push(DomainBound::closed(input(name), unit, lower, upper));
    }
    ValidityDomain { bounds, assumptions: vec![
        "Isolated untwisted symmetric NACA 0012 wing; attached flow; default mesh and wake".into(),
        "Induced-drag reduction only; no whole-aircraft interference, trim, stall, transonic shocks or experimental calibration".into(),
    ] }
}

impl AnalysisPlugin for PolarPlugin {
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
