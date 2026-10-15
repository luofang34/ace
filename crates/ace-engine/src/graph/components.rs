use super::{Component, Node};
use crate::EngineResult;
use ace_model::{ModelError, VehicleModel};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn compile(
    model: &VehicleModel,
    nodes: &BTreeMap<String, Node>,
    dependencies: &BTreeMap<String, BTreeSet<String>>,
) -> EngineResult<Vec<Component>> {
    let mut components = Vec::new();
    let mut used = BTreeSet::new();
    for group in strongly_connected(dependencies) {
        let cyclic = group.len() > 1
            || group
                .first()
                .is_some_and(|id| dependencies.get(id).is_some_and(|deps| deps.contains(id)));
        let outputs: BTreeSet<_> = group
            .iter()
            .filter_map(|id| nodes.get(id))
            .flat_map(|node| node.outputs.iter().cloned())
            .collect();
        let matches: Vec<_> = model
            .closures
            .iter()
            .filter(|(_, closure)| {
                closure.tolerances.keys().cloned().collect::<BTreeSet<_>>() == outputs
            })
            .collect();
        if cyclic && matches.len() != 1 {
            return Err(ModelError::invalid(
                format!("{outputs:?}"),
                "cycle requires exactly one complete closure declaration",
            )
            .into());
        }
        let closure = if let Some((id, _)) = matches.first() {
            if !cyclic {
                return Err(ModelError::invalid(*id, "closure does not describe a cycle").into());
            }
            for output in &outputs {
                if model.parameter(output)?.value.is_none() {
                    return Err(ModelError::invalid(
                        output,
                        "closure output requires an initial seed",
                    )
                    .into());
                }
            }
            used.insert((*id).clone());
            Some((*id).clone())
        } else {
            None
        };
        components.push(Component {
            nodes: group,
            closure,
        });
    }
    if used.len() != model.closures.len() {
        return Err(ModelError::invalid(
            "closures",
            "closure must match all outputs of one connected cycle",
        )
        .into());
    }
    Ok(components)
}

fn strongly_connected(dependencies: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for root in dependencies.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((name, expanded)) = stack.pop() {
            if expanded {
                order.push(name);
            } else if visited.insert(name.clone()) {
                stack.push((name.clone(), true));
                if let Some(children) = dependencies.get(&name) {
                    stack.extend(children.iter().rev().map(|child| (child.clone(), false)));
                }
            }
        }
    }
    let mut reverse: BTreeMap<String, BTreeSet<String>> = dependencies
        .keys()
        .map(|id| (id.clone(), BTreeSet::new()))
        .collect();
    for (name, children) in dependencies {
        for child in children {
            reverse
                .entry(child.clone())
                .or_default()
                .insert(name.clone());
        }
    }
    visited.clear();
    let mut groups = Vec::new();
    for root in order.iter().rev() {
        if !visited.insert(root.clone()) {
            continue;
        }
        let mut group = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(name) = stack.pop() {
            group.push(name.clone());
            if let Some(children) = reverse.get(&name) {
                for child in children.iter().rev() {
                    if visited.insert(child.clone()) {
                        stack.push(child.clone());
                    }
                }
            }
        }
        group.sort();
        groups.push(group);
    }
    groups.reverse();
    groups
}
