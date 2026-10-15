use super::Trace;
use ace_model::{AnalysisResources, Binding, VehicleModel};

pub(super) fn build(trace: &mut Trace, model: &VehicleModel) {
    geometry(trace, model);
    masses(trace, model);
    missions(trace, model);
    operations(trace, model);
}

fn geometry(trace: &mut Trace, model: &VehicleModel) {
    for (id, geometry) in &model.geometry {
        let key = trace.node("geometry", id, &geometry.name);
        for (binding, _) in geometry.bindings() {
            link_binding(trace, binding, &key);
        }
        if let Some(parent) = &geometry.parent {
            trace.link(format!("geometry:{parent}"), &key, "contains");
        }
        if let Some(frame) = &geometry.placement.relative_to {
            trace.link(format!("geometry:{frame}"), &key, "coordinate_frame");
        }
        for artifact in &geometry.artifacts {
            let id = trace.node("artifact", &artifact.digest, &artifact.schema);
            trace.link(id, &key, "geometry_content");
        }
    }
    for (id, clearance) in &model.clearances {
        let key = trace.node("clearance", id, id);
        trace.link(format!("geometry:{}", clearance.first), &key, "checked");
        trace.link(format!("geometry:{}", clearance.second), &key, "checked");
        link_binding(trace, &clearance.minimum, &key);
    }
    for (id, section) in &model.sections {
        let key = trace.node("section", id, id);
        link_binding(trace, &section.station, &key);
        for geometry in &section.geometry {
            trace.link(format!("geometry:{geometry}"), &key, "sectioned");
        }
    }
}

fn masses(trace: &mut Trace, model: &VehicleModel) {
    for (id, item) in &model.mass_items {
        let key = trace.node("mass", id, id);
        for binding in [&item.mass]
            .into_iter()
            .chain(&item.cg)
            .chain(&item.capacity)
        {
            link_binding(trace, binding, &key);
        }
        for station in &item.fill_curve {
            for binding in &station.cg {
                link_binding(trace, binding, &key);
            }
        }
        if let Some(inertia) = &item.inertia {
            for binding in [
                &inertia.xx,
                &inertia.yy,
                &inertia.zz,
                &inertia.xy,
                &inertia.xz,
                &inertia.yz,
            ] {
                link_binding(trace, binding, &key);
            }
        }
        if let Some(geometry) = &item.geometry {
            trace.link(format!("geometry:{geometry}"), &key, "mass_distribution");
        }
        if let Some(frame) = &item.frame {
            trace.link(format!("geometry:{frame}"), &key, "coordinate_frame");
        }
    }
    for (id, case) in &model.loading_cases {
        let key = trace.node("loading", id, id);
        for (item, binding) in &case.masses {
            trace.link(format!("mass:{item}"), &key, "loads");
            link_binding(trace, binding, &key);
        }
    }
}

fn missions(trace: &mut Trace, model: &VehicleModel) {
    for (id, mission) in &model.missions {
        let key = trace.node("mission", id, &mission.name);
        if let Some(loading) = &mission.loading {
            trace.link(format!("loading:{loading}"), &key, "initial_loading");
        }
        for stage in mission.all_stages() {
            trace.link(format!("analysis:{}", stage.analysis), &key, "phase");
            for tank in &stage.fuel_sequence {
                trace.link(format!("mass:{tank}"), &key, "consumes_fuel");
            }
            for item in stage.payload_release.keys() {
                trace.link(format!("mass:{item}"), &key, "releases");
            }
            for binding in stage.inputs.values().chain(stage.payload_release.values()) {
                link_binding(trace, binding, &key);
            }
        }
        for binding in mission.conditions.values() {
            link_binding(trace, binding, &key);
        }
    }
}

fn operations(trace: &mut Trace, model: &VehicleModel) {
    if let Some(case) = &model.operations {
        let key = trace.node("operations", "case", "Operating case");
        for (binding, _) in case.bindings() {
            link_binding(trace, binding, &key);
        }
    }
}

pub(super) fn consumption(trace: &mut Trace, analysis: &str, resources: &AnalysisResources) {
    for id in resources.geometry.keys().chain(resources.frames.keys()) {
        trace.link(format!("geometry:{id}"), analysis, "resource");
    }
    for id in resources.mass_items.keys() {
        trace.link(format!("mass:{id}"), analysis, "resource");
    }
    for id in resources.loading_cases.keys() {
        trace.link(format!("loading:{id}"), analysis, "resource");
    }
    for id in resources.missions.keys() {
        trace.link(format!("mission:{id}"), analysis, "resource");
    }
    for id in resources.clearances.keys() {
        trace.link(format!("clearance:{id}"), analysis, "resource");
    }
    for id in resources.sections.keys() {
        trace.link(format!("section:{id}"), analysis, "resource");
    }
    if resources.operations.is_some() {
        trace.link("operations:case", analysis, "resource");
    }
}

fn link_binding(trace: &mut Trace, binding: &Binding, key: &str) {
    if let Some(parameter) = binding.parameter() {
        trace.link(format!("parameter:{parameter}"), key, "defines");
    }
}
