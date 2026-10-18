use super::{PluginSpec, Registry};
use crate::{EngineError, EngineResult};
use ace_model::{Analysis, AnalysisMode, ContractError, DomainVariable, ScalarPort, VehicleModel};
use std::collections::BTreeMap;

impl PluginSpec {
    /// Validate a serializable scalar or state-phase contract before registration.
    pub fn validate(&self) -> Result<(), ContractError> {
        for identity in [
            &self.identity.id,
            &self.identity.version,
            &self.identity.code_revision,
        ] {
            if identity.trim().is_empty() {
                return Err(ContractError::invalid("identity", "empty solver identity"));
            }
        }
        if self.mode == AnalysisMode::Phase && !self.outputs.is_empty() {
            return Err(ContractError::invalid(
                &self.identity.id,
                "phase outputs belong to its trajectory",
            ));
        }
        for (name, port) in self.inputs.iter().chain(&self.outputs) {
            if name.trim().is_empty() {
                return Err(ContractError::invalid(&self.identity.id, "empty port name"));
            }
            port.validate(name)?;
        }
        self.domain.validate()?;
        for bound in &self.domain.bounds {
            match &bound.variable {
                DomainVariable::Input(name) => {
                    let port = self.inputs.get(name).ok_or_else(|| {
                        ContractError::invalid(name, "domain references an undeclared input")
                    })?;
                    if !bound.unit.compatible(port.unit) {
                        return Err(ContractError::invalid(
                            name,
                            "domain and input dimensions differ",
                        ));
                    }
                }
                DomainVariable::Condition(name) if self.mode != AnalysisMode::Phase => {
                    return Err(ContractError::invalid(
                        name,
                        "scalar solver cannot consume phase conditions",
                    ));
                }
                DomainVariable::Condition(_) => {}
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        &self,
        id: &str,
        analysis: &Analysis,
        model: &VehicleModel,
    ) -> Result<(), ContractError> {
        if self.mode != analysis.mode
            || !self.inputs.keys().eq(analysis.inputs.keys())
            || !self.outputs.keys().eq(analysis.outputs.keys())
        {
            return Err(ContractError::invalid(
                id,
                "invocation mode or named ports do not match",
            ));
        }
        for (ports, bindings) in [
            (&self.inputs, &analysis.inputs),
            (&self.outputs, &analysis.outputs),
        ] {
            for ((port, schema), parameter) in ports.iter().zip(bindings.values()) {
                if !schema.unit.compatible(model.parameter(parameter)?.unit) {
                    return Err(ContractError::invalid(
                        format!("{id}.{port}"),
                        format!("dimensions differ from {parameter}"),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Registry {
    /// Resolve installed contracts and check direct producer/consumer compatibility.
    /// Missing solvers remain unavailable; they never acquire an inferred contract.
    pub fn validate_model(&self, model: &VehicleModel) -> EngineResult<()> {
        let mut producers = BTreeMap::new();
        for (id, analysis) in &model.analyses {
            if let Some(plugin) = self.get(&analysis.plugin, &analysis.version) {
                plugin
                    .spec()
                    .bind(id, analysis, model)
                    .map_err(|source| error(id, source))?;
                for (port, parameter) in &analysis.outputs {
                    if let Some(schema) = plugin.spec().outputs.get(port) {
                        producers.insert(parameter, schema);
                    }
                }
            }
        }
        for (id, analysis) in &model.analyses {
            self.check_connections(id, analysis, model, &producers)?;
        }
        Ok(())
    }

    /// Frozen contracts for installed analyses, keyed by canonical analysis identifier.
    pub fn contracts(&self, model: &VehicleModel) -> BTreeMap<String, PluginSpec> {
        model
            .analyses
            .iter()
            .filter_map(|(id, analysis)| {
                self.get(&analysis.plugin, &analysis.version)
                    .map(|plugin| (id.clone(), plugin.spec().clone()))
            })
            .collect()
    }

    fn check_connections(
        &self,
        id: &str,
        analysis: &Analysis,
        model: &VehicleModel,
        producers: &BTreeMap<&String, &ScalarPort>,
    ) -> EngineResult<()> {
        let Some(plugin) = self.get(&analysis.plugin, &analysis.version) else {
            return Ok(());
        };
        for (port, parameter) in &analysis.inputs {
            if plugin
                .spec()
                .inputs
                .get(port)
                .is_some_and(|schema| schema.meaning.is_some())
                && model.parameter(parameter)?.expression.is_some()
            {
                return Err(error(
                    id,
                    ContractError::invalid(
                        port,
                        "semantic ports require an independent input or a direct typed producer; expressions do not infer semantics",
                    ),
                ));
            }
            if let (Some(consumer), Some(producer)) =
                (plugin.spec().inputs.get(port), producers.get(parameter))
                && !consumer.accepts(producer)
            {
                return Err(error(
                    id,
                    ContractError::invalid(
                        port,
                        format!(
                            "incompatible producer for {parameter}: {producer:?}, consumer {consumer:?}"
                        ),
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn error(id: &str, source: ContractError) -> EngineError {
    EngineError::Contract {
        analysis: id.into(),
        source,
    }
}
