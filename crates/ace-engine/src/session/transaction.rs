use super::{Session, initial_values};
use crate::graph::{Graph, Node, NodeKind};
use crate::{EngineResult, record::digest};
use ace_model::{AnalysisResources, ModelEdit, VehicleModel};
use std::collections::BTreeSet;

impl Session {
    /// Atomically apply typed structural edits, recompile references and revoke affected values.
    /// Failure leaves the model, revisions, cached values and execution evidence unchanged.
    pub fn transact(&mut self, edits: &[ModelEdit]) -> EngineResult<Vec<String>> {
        let mut model = self.model.clone();
        for edit in edits {
            edit.apply(&mut model)?;
        }
        if model == self.model {
            return Ok(Vec::new());
        }
        model.revision = self.model.revision.wrapping_add(1);
        for (id, geometry) in &mut model.geometry {
            if self.model.geometry.get(id) != Some(geometry) {
                geometry.revision = self
                    .model
                    .geometry
                    .get(id)
                    .map_or(0, |old| old.revision.wrapping_add(1));
            }
        }
        for (id, parameter) in &mut model.parameters {
            if self.model.parameters.get(id) != Some(parameter) {
                parameter.revision = self
                    .model
                    .parameters
                    .get(id)
                    .map_or(0, |old| old.revision.wrapping_add(1));
            }
        }
        let graph = Graph::compile(&model)?;
        let model_digest = digest(&model)?;
        let dirty = affected(self, &model, &graph)?;
        let mut values = initial_values(&model, &graph)?;
        for (id, value) in &mut values {
            if model.parameters.get(id) == self.model.parameters.get(id)
                && graph.producers.get(id) == self.graph.producers.get(id)
                && let Some(previous) = self.values.get(id)
            {
                *value = previous.clone();
            }
        }
        self.model = model;
        self.model_digest = model_digest;
        self.graph = graph;
        self.values = values;
        self.dirty.retain(|id| self.graph.nodes.contains_key(id));
        self.closures
            .retain(|id, _| self.model.closures.contains_key(id));
        self.invalidate(&dirty);
        Ok(dirty.into_iter().collect())
    }
}

fn affected(
    session: &Session,
    model: &VehicleModel,
    graph: &Graph,
) -> EngineResult<BTreeSet<String>> {
    let mut changed: BTreeSet<_> = model
        .parameters
        .iter()
        .filter(|(id, parameter)| session.model.parameters.get(*id) != Some(*parameter))
        .map(|(id, _)| id.clone())
        .collect();
    let mut dirty: BTreeSet<_> = session
        .dirty
        .iter()
        .filter(|id| graph.nodes.contains_key(*id))
        .cloned()
        .collect();
    for (id, node) in &graph.nodes {
        let old = session
            .graph
            .nodes
            .get(id)
            .map(|node| signature(&session.model, node))
            .transpose()?;
        if old.as_deref() != Some(&signature(model, node)?) || !node.outputs.is_disjoint(&changed) {
            dirty.insert(id.clone());
        }
    }
    for component in &graph.components {
        if component
            .closure
            .as_ref()
            .is_some_and(|id| model.closures.get(id) != session.model.closures.get(id))
        {
            dirty.extend(component.nodes.iter().cloned());
        }
    }
    for id in &dirty {
        if let Some(node) = graph.nodes.get(id) {
            changed.extend(node.outputs.iter().cloned());
        }
    }
    dirty.extend(graph.affected(&changed));
    Ok(dirty)
}

fn signature(model: &VehicleModel, node: &Node) -> EngineResult<String> {
    match &node.kind {
        NodeKind::Expression(id) => digest(model.parameter(id)?),
        NodeKind::Analysis(id) => {
            let analysis = model
                .analyses
                .get(id)
                .ok_or_else(|| ace_model::ModelError::UnknownReference(id.clone()))?;
            digest(&(analysis, AnalysisResources::collect(model, analysis)?))
        }
    }
}

#[cfg(test)]
mod tests;
