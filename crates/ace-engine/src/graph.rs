use crate::EngineResult;
use ace_model::{ModelError, VehicleModel};
use std::collections::{BTreeMap, BTreeSet};

mod components;

#[derive(Debug, Clone)]
pub(crate) enum NodeKind {
    Expression(String),
    Analysis(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub inputs: BTreeSet<String>,
    pub outputs: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Component {
    pub nodes: Vec<String>,
    pub closure: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Graph {
    pub nodes: BTreeMap<String, Node>,
    pub producers: BTreeMap<String, String>,
    pub components: Vec<Component>,
}

impl Graph {
    pub fn compile(model: &VehicleModel) -> EngineResult<Self> {
        model.validate()?;
        let nodes = nodes(model)?;
        let mut producers = BTreeMap::new();
        for node in nodes.values() {
            for output in &node.outputs {
                if producers.insert(output.clone(), node.id.clone()).is_some() {
                    return Err(ModelError::invalid(output, "multiple parameter producers").into());
                }
            }
        }
        let dependencies = nodes
            .iter()
            .map(|(id, node)| {
                let deps = node
                    .inputs
                    .iter()
                    .filter_map(|input| producers.get(input).cloned())
                    .collect();
                (id.clone(), deps)
            })
            .collect();
        let components = components::compile(model, &nodes, &dependencies)?;
        for variable in &model.design_variables {
            if producers.contains_key(&variable.parameter) {
                return Err(ModelError::invalid(
                    &variable.parameter,
                    "design variable must be an independent input",
                )
                .into());
            }
        }
        Ok(Self {
            nodes,
            producers,
            components,
        })
    }

    pub fn affected(&self, parameters: &BTreeSet<String>) -> BTreeSet<String> {
        let mut changed = parameters.clone();
        let mut dirty = BTreeSet::new();
        loop {
            let next: Vec<_> = self
                .nodes
                .values()
                .filter(|node| !dirty.contains(&node.id) && !node.inputs.is_disjoint(&changed))
                .collect();
            if next.is_empty() {
                return dirty;
            }
            for node in next {
                dirty.insert(node.id.clone());
                changed.extend(node.outputs.iter().cloned());
            }
        }
    }
}

fn nodes(model: &VehicleModel) -> EngineResult<BTreeMap<String, Node>> {
    let mut nodes = BTreeMap::new();
    for (name, parameter) in &model.parameters {
        if let Some(expression) = &parameter.expression {
            let id = format!("expression:{name}");
            nodes.insert(
                id.clone(),
                Node {
                    id,
                    kind: NodeKind::Expression(name.clone()),
                    inputs: expression.dependencies()?,
                    outputs: BTreeSet::from([name.clone()]),
                },
            );
        }
    }
    for (name, analysis) in &model.analyses {
        if analysis.mode == ace_model::AnalysisMode::Phase {
            continue;
        }
        let mut inputs: BTreeSet<_> = analysis.inputs.values().cloned().collect();
        inputs.extend(ace_model::AnalysisResources::collect(model, analysis)?.dependencies());
        let id = format!("analysis:{name}");
        nodes.insert(
            id.clone(),
            Node {
                id,
                kind: NodeKind::Analysis(name.clone()),
                inputs,
                outputs: analysis.outputs.values().cloned().collect(),
            },
        );
    }
    Ok(nodes)
}
