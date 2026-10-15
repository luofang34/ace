use ace_engine::{EngineResult, Margin, Snapshot};
use ace_model::{AnalysisResources, VehicleModel};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

mod resources;

/// Navigable provenance object; IDs contain an explicit domain prefix.
#[derive(Debug, Clone, Serialize)]
pub struct TraceNode {
    /// Stable navigation identifier.
    pub id: String,
    /// Object kind, e.g. parameter, geometry, analysis, run, requirement, or margin.
    pub kind: String,
    /// Human-readable name.
    pub label: String,
}

/// Directed semantic relationship, traversable in either direction by a client.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraceLink {
    /// Source object identifier.
    pub from: String,
    /// Target object identifier.
    pub to: String,
    /// Meaning of the relationship.
    pub relation: String,
}

/// Complete current provenance navigation graph.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Trace {
    /// Nodes indexed by stable navigation ID.
    pub nodes: BTreeMap<String, TraceNode>,
    /// Deduplicated directed relationships.
    pub links: BTreeSet<TraceLink>,
}

impl Trace {
    fn node(&mut self, kind: &str, id: &str, label: &str) -> String {
        let key = format!("{kind}:{id}");
        self.nodes.insert(
            key.clone(),
            TraceNode {
                id: key.clone(),
                kind: kind.into(),
                label: label.into(),
            },
        );
        key
    }
    fn link(&mut self, from: impl Into<String>, to: impl Into<String>, relation: &str) {
        self.links.insert(TraceLink {
            from: from.into(),
            to: to.into(),
            relation: relation.into(),
        });
    }
}

pub(crate) fn build(
    model: &VehicleModel,
    snapshot: &Snapshot,
    margins: &BTreeMap<String, Margin>,
) -> EngineResult<Trace> {
    let mut trace = Trace::default();
    declarations(&mut trace, model)?;
    resources::build(&mut trace, model);
    for (id, closure) in &model.closures {
        let key = trace.node("closure", id, id);
        for parameter in closure.tolerances.keys() {
            trace.link(format!("parameter:{parameter}"), &key, "convergence_metric");
            trace.link(&key, format!("parameter:{parameter}"), "closes");
        }
    }
    for (id, requirement) in &model.requirements {
        let key = trace.node("requirement", id, &requirement.name);
        trace.link(
            &key,
            format!("parameter:{}", requirement.metric),
            "constrains",
        );
        for geometry in &requirement.geometry {
            trace.link(&key, format!("geometry:{geometry}"), "applies_to");
        }
        for condition in requirement.condition.keys() {
            trace.link(format!("parameter:{condition}"), &key, "condition");
        }
    }
    for (id, margin) in margins {
        let key = trace.node("margin", id, id);
        trace.link(format!("requirement:{id}"), &key, "evaluated_as");
        trace.link(
            format!("parameter:{}", margin.parameter),
            &key,
            "measured_by",
        );
        if let Some(run) = &margin.run_id {
            trace.link(format!("run:{run}"), &key, "supports");
        }
    }
    for (id, objective) in &model.objectives {
        let key = trace.node("objective", id, id);
        trace.link(format!("parameter:{}", objective.metric), key, "optimizes");
    }
    calculations(&mut trace, snapshot);
    Ok(trace)
}

fn declarations(trace: &mut Trace, model: &VehicleModel) -> EngineResult<()> {
    for (id, parameter) in &model.parameters {
        let key = trace.node("parameter", id, id);
        if let Some(expression) = &parameter.expression {
            let expression_key = trace.node("expression", id, id);
            trace.link(&expression_key, &key, "computes");
            for input in expression.dependencies()? {
                trace.link(format!("parameter:{input}"), &expression_key, "input");
            }
        }
    }
    for (id, analysis) in &model.analyses {
        let key = trace.node("analysis", id, id);
        let resources = AnalysisResources::collect(model, analysis)?;
        for input in analysis
            .inputs
            .values()
            .cloned()
            .chain(resources.dependencies())
        {
            trace.link(format!("parameter:{input}"), &key, "input");
        }
        for output in analysis.outputs.values() {
            trace.link(&key, format!("parameter:{output}"), "computes");
        }
        for id in &analysis.requirements {
            trace.link(format!("requirement:{id}"), &key, "motivates");
        }
        resources::consumption(trace, &key, &resources);
    }
    Ok(())
}

fn calculations(trace: &mut Trace, snapshot: &Snapshot) {
    for run in &snapshot.runs {
        let label = format!("{} · {}", run.node, run.id.get(..10).unwrap_or(&run.id));
        let key = trace.node("run", &run.id, &label);
        if !trace.nodes.contains_key(&run.node) {
            trace.nodes.insert(
                run.node.clone(),
                TraceNode {
                    id: run.node.clone(),
                    kind: "closure".into(),
                    label: run.node.clone(),
                },
            );
        }
        trace.link(&run.node, &key, "executed_as");
        for artifact in &run.artifacts {
            let id = trace.node("artifact", &artifact.digest, &artifact.schema);
            trace.link(&key, id, "produced_artifact");
        }
        for input in run.inputs.values() {
            trace.link(format!("parameter:{}", input.parameter), &key, "consumed");
            if let Some(id) = &input.run_id {
                trace.link(format!("run:{id}"), &key, "consumed_result");
            }
        }
        for output in run.outputs.keys() {
            trace.link(&key, format!("parameter:{output}"), "produced");
        }
    }
}
