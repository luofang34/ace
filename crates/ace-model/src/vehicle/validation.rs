use crate::{Binding, MassKind, ModelError, ModelResult, Quantity, Relation, Unit, VehicleModel};
use std::collections::{BTreeMap, BTreeSet};
mod placement;
use placement::placement_chain;

pub(super) fn validate(model: &VehicleModel) -> ModelResult<()> {
    for (name, analysis) in &model.analyses {
        crate::AnalysisResources::collect(model, analysis)?;
        if analysis.plugin.is_empty()
            || analysis.version.is_empty()
            || (analysis.mode == crate::AnalysisMode::Scalar && analysis.outputs.is_empty())
        {
            return Err(ModelError::invalid(
                name,
                "plugin, version, and outputs are required",
            ));
        }
        if analysis.mode == crate::AnalysisMode::Phase && !analysis.outputs.is_empty() {
            return Err(ModelError::invalid(
                name,
                "phase outputs belong to mission trajectory evidence",
            ));
        }
        let unique: BTreeSet<_> = analysis.outputs.values().collect();
        if unique.len() != analysis.outputs.len() {
            return Err(ModelError::invalid(
                name,
                "output parameters must be unique",
            ));
        }
        for parameter in analysis.inputs.values().chain(analysis.outputs.values()) {
            model.parameter(parameter)?;
        }
        for name in &analysis.geometry {
            reference(&model.geometry, name)?;
        }
        for name in &analysis.requirements {
            reference(&model.requirements, name)?;
        }
        for name in &analysis.resources.catalog {
            reference(&model.catalog, name)?;
        }
    }
    for (id, entry) in &model.catalog {
        entry.validate(id)?;
    }
    geometry(model)?;
    mass(model)?;
    missions(model)?;
    requirements(model)?;
    operations(model)?;
    if let Some(map) = &model.capabilities {
        map.validate(model)?;
    }
    if let Some(production) = &model.production {
        production.validate(model)?;
        for (value, unit) in production.bindings() {
            binding(model, value, unit)?;
        }
    }
    for (name, closure) in &model.closures {
        if closure.tolerances.is_empty()
            || closure.max_iterations == 0
            || closure.max_iterations > 10000
            || !closure.relaxation.is_finite()
            || !(0.0..=1.0).contains(&closure.relaxation)
            || closure.relaxation == 0.0
        {
            return Err(ModelError::invalid(
                name,
                "invalid closure bounds or relaxation",
            ));
        }
        for (parameter, tolerance) in &closure.tolerances {
            let unit = model.parameter(parameter)?.unit;
            if tolerance.absolute.to(unit)?.value <= 0.0
                || !tolerance.relative.is_finite()
                || !(0.0..1.0).contains(&tolerance.relative)
            {
                return Err(ModelError::invalid(name, "invalid convergence tolerance"));
            }
        }
    }
    Ok(())
}

fn binding(model: &VehicleModel, value: &Binding, unit: Unit) -> ModelResult<()> {
    match value {
        Binding::Parameter(name) => {
            Quantity::new(1.0, model.parameter(name)?.unit)?.to(unit)?;
        }
        Binding::Constant(value) => {
            value.to(unit)?;
        }
    }
    Ok(())
}

fn reference<T>(map: &BTreeMap<String, T>, name: &str) -> ModelResult<()> {
    if !map.contains_key(name) {
        return Err(ModelError::UnknownReference(name.to_owned()));
    }
    Ok(())
}

fn geometry(model: &VehicleModel) -> ModelResult<()> {
    for (name, item) in &model.geometry {
        item.placement.validate(name)?;
        for artifact in &item.artifacts {
            artifact.validate()?;
        }
        placement_chain(model, name)?;
        for (value, unit) in item.bindings() {
            binding(model, value, unit)?;
        }
        let mut seen = BTreeSet::from([name.as_str()]);
        let mut parent = item.parent.as_deref();
        while let Some(name) = parent {
            reference(&model.geometry, name)?;
            if !seen.insert(name) {
                return Err(ModelError::invalid(name, "geometry parent cycle"));
            }
            parent = model
                .geometry
                .get(name)
                .and_then(|item| item.parent.as_deref());
        }
    }
    for (name, clearance) in &model.clearances {
        reference(&model.geometry, &clearance.first)?;
        reference(&model.geometry, &clearance.second)?;
        if clearance.first == clearance.second {
            return Err(ModelError::invalid(name, "clearance needs two objects"));
        }
        binding(model, &clearance.minimum, Unit::Meter)?;
    }
    for (name, section) in &model.sections {
        if section.axis > 2 {
            return Err(ModelError::invalid(name, "section axis must be 0, 1 or 2"));
        }
        binding(model, &section.station, Unit::Meter)?;
        for name in &section.geometry {
            reference(&model.geometry, name)?;
        }
    }
    Ok(())
}

fn mass(model: &VehicleModel) -> ModelResult<()> {
    for (name, item) in &model.mass_items {
        binding(model, &item.mass, Unit::Kilogram)?;
        for coordinate in &item.cg {
            binding(model, coordinate, Unit::Meter)?;
        }
        if let Some(id) = &item.geometry {
            reference(&model.geometry, id)?;
        }
        if let Some(id) = &item.frame {
            reference(&model.geometry, id)?;
        }
        if let Some(capacity) = &item.capacity {
            binding(model, capacity, Unit::Kilogram)?;
        }
        if let Some(inertia) = &item.inertia {
            for value in [
                &inertia.xx,
                &inertia.yy,
                &inertia.zz,
                &inertia.xy,
                &inertia.xz,
                &inertia.yz,
            ] {
                binding(model, value, Unit::Inertia)?;
            }
        }
        if !item.fill_curve.is_empty() {
            if item.capacity.is_none()
                || item.fill_curve.first().map(|station| station.fraction) != Some(0.0)
                || item.fill_curve.last().map(|station| station.fraction) != Some(1.0)
            {
                return Err(ModelError::invalid(
                    name,
                    "fill curve requires capacity and 0/1 endpoints",
                ));
            }
            let mut previous = -1.0;
            for station in &item.fill_curve {
                if !station.fraction.is_finite()
                    || station.fraction <= previous
                    || station.fraction > 1.0
                {
                    return Err(ModelError::invalid(
                        name,
                        "fill stations must be strictly increasing",
                    ));
                }
                previous = station.fraction;
                for coordinate in &station.cg {
                    binding(model, coordinate, Unit::Meter)?;
                }
            }
        }
    }
    for loading in model.loading_cases.values() {
        for (id, value) in &loading.masses {
            reference(&model.mass_items, id)?;
            binding(model, value, Unit::Kilogram)?;
        }
    }
    Ok(())
}

fn missions(model: &VehicleModel) -> ModelResult<()> {
    for (name, mission) in &model.missions {
        for limit in &mission.cg_envelope {
            binding(model, &limit.mass, Unit::Kilogram)?;
            binding(model, &limit.minimum, Unit::Meter)?;
            binding(model, &limit.maximum, Unit::Meter)?;
        }
        if mission.stages.is_empty() {
            return Err(ModelError::invalid(name, "mission has no stages"));
        }
        if let Some(loading) = &mission.loading {
            reference(&model.loading_cases, loading)?;
        }
        for (branch_id, branch) in &mission.branches {
            if branch.stages.is_empty()
                || branch
                    .from_stage
                    .as_ref()
                    .is_some_and(|id| !mission.stages.iter().any(|stage| &stage.id == id))
            {
                return Err(ModelError::invalid(
                    branch_id,
                    "branch needs stages and a nominal entry boundary",
                ));
            }
        }
        let mut ids = BTreeSet::new();
        for stage in mission.all_stages() {
            if !ids.insert(&stage.id) {
                return Err(ModelError::invalid(name, "duplicate mission stage"));
            }
            reference(&model.analyses, &stage.analysis)?;
            for id in &stage.fuel_sequence {
                reference(&model.mass_items, id)?;
                if model
                    .mass_items
                    .get(id)
                    .is_none_or(|item| item.kind != MassKind::Fuel)
                {
                    return Err(ModelError::invalid(
                        id,
                        "fuel sequence requires a fuel mass item",
                    ));
                }
            }
            for (id, value) in &stage.payload_release {
                reference(&model.mass_items, id)?;
                if model
                    .mass_items
                    .get(id)
                    .is_none_or(|item| item.kind != MassKind::Payload)
                {
                    return Err(ModelError::invalid(
                        id,
                        "release requires a payload mass item",
                    ));
                }
                binding(model, value, Unit::Kilogram)?;
            }
            for value in stage.inputs.values().chain(mission.conditions.values()) {
                match value {
                    Binding::Parameter(id) => {
                        model.parameter(id)?;
                    }
                    Binding::Constant(quantity) => {
                        quantity.dimensional()?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn requirements(model: &VehicleModel) -> ModelResult<()> {
    for (name, requirement) in &model.requirements {
        let unit = model.parameter(&requirement.metric)?.unit;
        requirement.target.to(unit)?;
        if requirement.relation == Relation::Eq && requirement.tolerance.is_none() {
            return Err(ModelError::invalid(
                name,
                "equality requires an explicit tolerance",
            ));
        }
        if let Some(tolerance) = requirement.tolerance
            && tolerance.to(unit)?.value < 0.0
        {
            return Err(ModelError::invalid(name, "negative tolerance"));
        }
        for (id, value) in &requirement.condition {
            value.to(model.parameter(id)?.unit)?;
        }
        for id in &requirement.geometry {
            reference(&model.geometry, id)?;
        }
        if requirement
            .confidence
            .is_some_and(|value| !(value > 0.0 && value < 1.0))
        {
            return Err(ModelError::invalid(
                name,
                "confidence must lie strictly between zero and one",
            ));
        }
    }
    for objective in model.objectives.values() {
        model.parameter(&objective.metric)?;
    }
    let mut seen = BTreeSet::new();
    for variable in &model.design_variables {
        let parameter = model.parameter(&variable.parameter)?;
        let lower = variable.lower.to(parameter.unit)?.value;
        let upper = variable.upper.to(parameter.unit)?.value;
        if !seen.insert(&variable.parameter) || lower >= upper {
            return Err(ModelError::invalid(
                &variable.parameter,
                "duplicate variable or reversed bounds",
            ));
        }
        parameter.check_value(variable.lower)?;
        parameter.check_value(variable.upper)?;
    }
    Ok(())
}

fn operations(model: &VehicleModel) -> ModelResult<()> {
    let Some(operations) = &model.operations else {
        return Ok(());
    };
    operations.validate_economics()?;
    for (value, unit) in operations.bindings() {
        binding(model, value, unit)?;
    }
    let mut costs = BTreeSet::new();
    for cost in &operations.costs {
        if !costs.insert(&cost.id) {
            return Err(ModelError::invalid(&cost.id, "duplicate cost item"));
        }
        binding(model, &cost.amount, Unit::Dollar)?;
    }
    let mut airports = BTreeSet::new();
    for airport in &operations.airports {
        if !airports.insert(&airport.id) {
            return Err(ModelError::invalid(&airport.id, "duplicate airport"));
        }
        binding(model, &airport.maximum_span, Unit::Meter)?;
        binding(model, &airport.runway, Unit::Meter)?;
        binding(model, &airport.movement_fee, Unit::Dollar)?;
    }
    for value in [&operations.span, &operations.required_runway]
        .into_iter()
        .flatten()
    {
        binding(model, value, Unit::Meter)?;
    }
    if let Some(revenue) = &operations.revenue_per_passenger {
        binding(model, revenue, Unit::Dollar)?;
    }
    Ok(())
}
