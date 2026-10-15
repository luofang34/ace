use crate::graph::{Node, NodeKind};
use crate::record::finalize;
use crate::{
    AnalysisContext, EngineResult, PluginOutput, Registry, RunRecord, SolverError, ValueRecord,
    ValueStatus,
};
use ace_model::{ModelError, Quantity, VehicleModel};
use std::collections::BTreeMap;

pub(crate) struct Executor<'a> {
    pub model: &'a VehicleModel,
    pub digest: &'a str,
    pub registry: &'a Registry,
}

pub(crate) struct Execution {
    pub values: BTreeMap<String, ValueRecord>,
    pub run: RunRecord,
}

impl Executor<'_> {
    pub fn execute(
        &self,
        node: &Node,
        values: &BTreeMap<String, ValueRecord>,
        iteration: Option<u32>,
    ) -> EngineResult<Execution> {
        let inputs: BTreeMap<_, _> = node
            .inputs
            .iter()
            .filter_map(|name| values.get(name).map(|value| (name.clone(), value.clone())))
            .collect();
        let mut run = self.evidence(node, inputs, iteration)?;
        let result = self.calculate(node, &run.inputs);
        match result {
            Ok(output) => {
                run.status = if output.approximate
                    || run
                        .inputs
                        .values()
                        .any(|value| value.status == ValueStatus::Approximate)
                {
                    ValueStatus::Approximate
                } else {
                    ValueStatus::Computed
                };
                run.outputs = output.values;
                run.artifacts = output.artifacts;
                run.details = output.details;
                if let Some(message) = output.failure {
                    run.status = ValueStatus::Failed;
                    run.outputs.clear();
                    run.message = Some(message);
                }
            }
            Err((status, message)) => {
                run.status = status;
                run.message = Some(message);
            }
        }
        let run = finalize(run)?;
        let mut values = BTreeMap::new();
        for name in &node.outputs {
            let parameter = self.model.parameter(name)?;
            values.insert(
                name.clone(),
                ValueRecord {
                    parameter: name.clone(),
                    status: run.status,
                    quantity: run.outputs.get(name).copied(),
                    run_id: Some(run.id.clone()),
                    revision: parameter.revision,
                    source: parameter.source.clone(),
                    message: run.message.clone(),
                },
            );
        }
        Ok(Execution { values, run })
    }

    fn evidence(
        &self,
        node: &Node,
        inputs: BTreeMap<String, ValueRecord>,
        iteration: Option<u32>,
    ) -> EngineResult<RunRecord> {
        let mut run = RunRecord {
            id: String::new(),
            node: node.id.clone(),
            model_digest: self.digest.into(),
            plugin: None,
            calculation: serde_json::Value::Null,
            inputs,
            outputs: BTreeMap::new(),
            status: ValueStatus::Unavailable,
            resources: Default::default(),
            details: serde_json::Value::Null,
            artifacts: Vec::new(),
            message: None,
            iteration,
        };
        match &node.kind {
            NodeKind::Expression(name) => {
                run.calculation = serde_json::to_value(&self.model.parameter(name)?.expression)?;
            }
            NodeKind::Analysis(name) => {
                if let Some(analysis) = self.model.analyses.get(name) {
                    run.calculation = serde_json::to_value(analysis)?;
                    run.plugin = self
                        .registry
                        .get(&analysis.plugin, &analysis.version)
                        .map(|plugin| plugin.spec().identity.clone());
                    run.resources = ace_model::AnalysisResources::collect(self.model, analysis)?;
                }
            }
        }
        Ok(run)
    }

    fn calculate(
        &self,
        node: &Node,
        inputs: &BTreeMap<String, ValueRecord>,
    ) -> Result<PluginOutput, (ValueStatus, String)> {
        if let NodeKind::Analysis(name) = &node.kind
            && let Some(analysis) = self.model.analyses.get(name)
            && self
                .registry
                .get(&analysis.plugin, &analysis.version)
                .is_none()
        {
            return Err((
                ValueStatus::Unavailable,
                format!(
                    "plugin {}@{} is not installed",
                    analysis.plugin, analysis.version
                ),
            ));
        }
        let missing: Vec<_> = node
            .inputs
            .iter()
            .filter(|name| inputs.get(*name).is_none_or(|value| !value.usable()))
            .collect();
        if !missing.is_empty() {
            return Err((
                ValueStatus::Blocked,
                format!("unavailable inputs: {missing:?}"),
            ));
        }
        let quantities = inputs
            .iter()
            .filter_map(|(name, value)| value.quantity.map(|q| (name.clone(), q)))
            .collect();
        self.compute(node, &quantities)
            .map_err(|error| (ValueStatus::Failed, error.to_string()))
    }

    fn compute(
        &self,
        node: &Node,
        inputs: &BTreeMap<String, Quantity>,
    ) -> Result<PluginOutput, SolverError> {
        let mut output = match &node.kind {
            NodeKind::Expression(name) => {
                let parameter = self.model.parameter(name)?;
                let expression = parameter
                    .expression
                    .as_ref()
                    .ok_or_else(|| ModelError::invalid(name, "missing expression"))?;
                PluginOutput {
                    values: BTreeMap::from([(
                        name.clone(),
                        expression.evaluate(inputs, parameter.unit)?,
                    )]),
                    approximate: false,
                    artifacts: Vec::new(),
                    details: serde_json::Value::Null,
                    failure: None,
                }
            }
            NodeKind::Analysis(name) => self.plugin(name, inputs)?,
        };
        for (name, value) in &mut output.values {
            *value = self.model.parameter(name)?.check_value(*value)?;
        }
        Ok(output)
    }

    fn plugin(
        &self,
        name: &str,
        values: &BTreeMap<String, Quantity>,
    ) -> Result<PluginOutput, SolverError> {
        let analysis = self
            .model
            .analyses
            .get(name)
            .ok_or_else(|| ModelError::UnknownReference(name.into()))?;
        let plugin = self
            .registry
            .get(&analysis.plugin, &analysis.version)
            .ok_or_else(|| SolverError::Failed(format!("missing plugin for {name}")))?;
        let spec = plugin.spec();
        if !spec.inputs.keys().eq(analysis.inputs.keys())
            || !spec.outputs.keys().eq(analysis.outputs.keys())
        {
            return Err(SolverError::Failed(format!(
                "port contract mismatch in {name}"
            )));
        }
        let mut inputs = BTreeMap::new();
        for ((port, parameter), unit) in analysis.inputs.iter().zip(spec.inputs.values()) {
            let value = values
                .get(parameter)
                .ok_or_else(|| ModelError::UnknownReference(parameter.clone()))?;
            inputs.insert(port.clone(), value.to(*unit)?);
        }
        for (parameter, unit) in analysis.outputs.values().zip(spec.outputs.values()) {
            Quantity::new(1.0, *unit)?.to(self.model.parameter(parameter)?.unit)?;
        }
        let geometry = AnalysisContext {
            resources: ace_model::AnalysisResources::collect(self.model, analysis)?,
            values,
            registry: self.registry,
        };
        let result = plugin.evaluate(&inputs, &geometry)?;
        self.registry.control().checkpoint()?;
        for artifact in &result.artifacts {
            artifact.validate()?;
        }
        if result.failure.is_none() && !spec.outputs.keys().eq(result.values.keys()) {
            return Err(SolverError::Failed(format!(
                "solver output contract mismatch in {name}"
            )));
        }
        let mut outputs = BTreeMap::new();
        for ((port, parameter), unit) in analysis
            .outputs
            .iter()
            .zip(spec.outputs.values())
            .filter(|_| result.failure.is_none())
        {
            let value = result
                .values
                .get(port)
                .ok_or_else(|| ModelError::UnknownReference(port.clone()))?;
            outputs.insert(parameter.clone(), value.to(*unit)?);
        }
        Ok(PluginOutput {
            values: outputs,
            approximate: result.approximate || spec.identity.fidelity == crate::Fidelity::Surrogate,
            artifacts: result.artifacts,
            details: result.details,
            failure: result.failure,
        })
    }
}
