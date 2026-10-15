use crate::execution::Executor;
use crate::graph::{Graph, NodeKind};
use crate::record::digest;
use crate::{
    AnalysisPlugin, ClosureResult, EngineResult, Registry, RunRecord, Snapshot, ValueRecord,
    ValueStatus,
};
use ace_model::{ModelError, Quantity, Source, VehicleModel};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod transaction;

/// Scoped authoritative design session with transactional edits and dirty propagation.
pub struct Session {
    model: VehicleModel,
    graph: Graph,
    registry: Registry,
    model_digest: String,
    dirty: BTreeSet<String>,
    values: BTreeMap<String, ValueRecord>,
    runs: Vec<RunRecord>,
    closures: BTreeMap<String, ClosureResult>,
}

impl Session {
    /// Fork the current model, dirty set, values, and immutable execution evidence.
    /// Subsequent edits invalidate only the fork's affected region.
    pub fn fork(&self) -> Self {
        Self {
            model: self.model.clone(),
            graph: self.graph.clone(),
            registry: self.registry.clone(),
            model_digest: self.model_digest.clone(),
            dirty: self.dirty.clone(),
            values: self.values.clone(),
            runs: self.runs.clone(),
            closures: self.closures.clone(),
        }
    }

    /// Create an independent design branch with the same installed solvers.
    /// Model edits and execution evidence are independent; installed adapters are shared.
    pub fn branch(&self) -> EngineResult<Self> {
        Self::new(self.model.clone(), self.registry.clone())
    }

    /// Create an independent branch with fast solvers and registered preview adapters.
    /// Expensive analyses without a preview adapter remain explicitly unavailable.
    pub fn preview_branch(&self) -> EngineResult<Self> {
        Self::new(self.model.clone(), self.registry.preview())
    }
    /// Validate and compile a vehicle model with an application-selected plugin registry.
    pub fn new(model: VehicleModel, registry: Registry) -> EngineResult<Self> {
        let graph = Graph::compile(&model)?;
        let values = initial_values(&model, &graph)?;
        let model_digest = digest(&model)?;
        let dirty = graph.nodes.keys().cloned().collect();
        Ok(Self {
            model,
            graph,
            registry,
            model_digest,
            dirty,
            values,
            runs: Vec::new(),
            closures: BTreeMap::new(),
        })
    }

    /// Inspect authoritative data; mutation is only available through validated transactions.
    pub fn model(&self) -> &VehicleModel {
        &self.model
    }

    /// Apply one atomic input edit and immediately revoke all affected output values.
    pub fn edit(
        &mut self,
        updates: &BTreeMap<String, Quantity>,
        source: Source,
    ) -> EngineResult<Vec<String>> {
        let mut model = self.model.clone();
        let mut changed = BTreeSet::new();
        for (name, value) in updates {
            if self.graph.producers.contains_key(name) {
                return Err(ModelError::invalid(name, "computed outputs cannot be edited").into());
            }
            let parameter = model
                .parameters
                .get_mut(name)
                .ok_or_else(|| ModelError::UnknownReference(name.clone()))?;
            let value = parameter.check_value(*value)?;
            if parameter
                .value
                .map(|old| old.to(parameter.unit))
                .transpose()?
                == Some(value)
                && parameter.source == source
            {
                continue;
            }
            parameter.value = Some(value);
            parameter.source = source.clone();
            parameter.revision = parameter.revision.wrapping_add(1);
            changed.insert(name.clone());
        }
        if changed.is_empty() {
            return Ok(Vec::new());
        }
        model.revision = model.revision.wrapping_add(1);
        model.validate()?;
        let model_digest = digest(&model)?;
        let initial = initial_values(&model, &self.graph)?;
        self.model = model;
        self.model_digest = model_digest;
        for name in &changed {
            if let Some(value) = initial.get(name) {
                self.values.insert(name.clone(), value.clone());
            }
        }
        let dirty = self.graph.affected(&changed);
        let affected = dirty.iter().cloned().collect();
        self.invalidate(&dirty);
        Ok(affected)
    }

    /// Install a deferred solver and invalidate only its affected graph region.
    pub fn register(&mut self, plugin: Arc<dyn AnalysisPlugin>) -> EngineResult<()> {
        let identity = plugin.spec().identity.clone();
        self.registry.register(plugin)?;
        let mut dirty = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        for (id, node) in &self.graph.nodes {
            if let NodeKind::Analysis(name) = &node.kind
                && let Some(analysis) = self.model.analyses.get(name)
            {
                let resources = ace_model::AnalysisResources::collect(&self.model, analysis)?;
                if std::iter::once(analysis)
                    .chain(resources.stage_analyses.values())
                    .any(|analysis| {
                        analysis.plugin == identity.id && analysis.version == identity.version
                    })
                {
                    dirty.insert(id.clone());
                    outputs.extend(node.outputs.iter().cloned());
                }
            }
        }
        dirty.extend(self.graph.affected(&outputs));
        self.invalidate(&dirty);
        Ok(())
    }

    fn invalidate(&mut self, nodes: &BTreeSet<String>) {
        self.dirty.extend(nodes.iter().cloned());
        for node in nodes.iter().filter_map(|id| self.graph.nodes.get(id)) {
            for output in &node.outputs {
                if let Some(record) = self.values.get_mut(output) {
                    record.status = ValueStatus::Stale;
                    record.quantity = None;
                    record.run_id = None;
                    record.message = None;
                }
            }
        }
        for component in &self.graph.components {
            if component.nodes.iter().any(|id| nodes.contains(id))
                && let Some(id) = &component.closure
            {
                self.closures.remove(id);
            }
        }
    }

    /// Recompute dirty components, keeping unmodified calculations and their run IDs.
    pub fn run(&mut self) -> EngineResult<Snapshot> {
        self.run_controlled(&crate::RunControl::default())
    }

    /// Recompute using one caller-owned cancellation token and deadline.
    pub fn run_controlled(&mut self, control: &crate::RunControl) -> EngineResult<Snapshot> {
        control.checkpoint()?;
        let registry = self.registry.controlled(control.clone());
        let executor = Executor {
            model: &self.model,
            digest: &self.model_digest,
            registry: &registry,
        };
        let mut executed = Vec::new();
        for component in &self.graph.components {
            control.checkpoint()?;
            if !component.nodes.iter().any(|id| self.dirty.contains(id)) {
                continue;
            }
            if let Some(id) = &component.closure {
                let outcome =
                    crate::closure::solve(id, component, &self.graph, &executor, &self.values)?;
                self.values.extend(outcome.values);
                self.runs.extend(outcome.runs);
                self.closures.insert(id.clone(), outcome.result);
            } else if let Some(node) = component
                .nodes
                .first()
                .and_then(|id| self.graph.nodes.get(id))
            {
                let outcome = executor.execute(node, &self.values, None)?;
                self.values.extend(outcome.values);
                self.runs.push(outcome.run);
            }
            executed.extend(component.nodes.iter().cloned());
            control.checkpoint()?;
            for id in &component.nodes {
                self.dirty.remove(id);
            }
        }
        let mut snapshot = self.snapshot();
        snapshot.executed = executed;
        Ok(snapshot)
    }

    /// Project state without running any analyses; stale quantities remain unavailable.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            build: crate::implementation_identity(),
            vehicle_id: self.model.id.clone(),
            revision: self.model.revision,
            model_digest: self.model_digest.clone(),
            values: self.values.clone(),
            runs: self.runs.clone(),
            closures: self.closures.clone(),
            executed: Vec::new(),
        }
    }
}

fn initial_values(
    model: &VehicleModel,
    graph: &Graph,
) -> EngineResult<BTreeMap<String, ValueRecord>> {
    let mut values = BTreeMap::new();
    for (name, parameter) in &model.parameters {
        let output = graph.producers.contains_key(name);
        let quantity = if output {
            None
        } else {
            parameter.value.map(|q| q.to(parameter.unit)).transpose()?
        };
        let status = if quantity.is_some() {
            ValueStatus::Input
        } else if output {
            ValueStatus::Stale
        } else {
            ValueStatus::Unavailable
        };
        values.insert(
            name.clone(),
            ValueRecord {
                parameter: name.clone(),
                status,
                quantity,
                run_id: None,
                revision: parameter.revision,
                source: parameter.source.clone(),
                message: None,
            },
        );
    }
    Ok(values)
}

#[cfg(test)]
mod tests;
