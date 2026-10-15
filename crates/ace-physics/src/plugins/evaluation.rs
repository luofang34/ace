use super::{Kind, input, result};
use crate::{PhysicsError, PhysicsResult, evaluate_geometry, evaluate_mass, evaluate_operations};
use ace_engine::{AnalysisContext, PluginOutput};
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn evaluate(
    kind: Kind,
    inputs: &BTreeMap<String, Quantity>,
    context: &AnalysisContext<'_>,
) -> PhysicsResult<PluginOutput> {
    match kind {
        Kind::ConceptGeometry => {
            let report = crate::evaluate_concept_geometry(&context.resources, context.values)?;
            result(
                &report,
                &[("part_area", report.part_area_m2, Unit::SquareMeter)],
                None,
                true,
            )
        }
        Kind::Geometry => geometry(context),
        Kind::Mass => mass(context),
        Kind::Mission => mission(context),
        Kind::Operations => operations(context, false),
        Kind::TripCost => operations(context, true),
        Kind::Phase => phase(inputs),
        Kind::Volume => measurement(context, true),
        Kind::WettedArea => measurement(context, false),
        Kind::FuelSizing => fuel_requirement(inputs, context),
        Kind::CgEnvelope => cg_envelope(context),
    }
}

fn cg_envelope(context: &AnalysisContext<'_>) -> PhysicsResult<PluginOutput> {
    if context.resources.missions.len() != 1 {
        return Err(PhysicsError::invalid(
            "CG envelope",
            "select exactly one mission",
        ));
    }
    let (id, mission) = context
        .resources
        .missions
        .iter()
        .next()
        .ok_or_else(|| PhysicsError::invalid("CG envelope", "missing mission"))?;
    let states =
        crate::evaluate_mission_with(id, &context.resources, context.values, context.registry)?;
    let report = crate::evaluate_cg_envelope(&states, &mission.cg_envelope, context.values)?;
    let margin = report.limiting.as_ref().and_then(|point| point.margin_m);
    result(
        &report,
        &[("margin", margin.unwrap_or(0.0), Unit::Meter)],
        report.failure.clone(),
        true,
    )
}

fn fuel_requirement(
    inputs: &BTreeMap<String, Quantity>,
    context: &AnalysisContext<'_>,
) -> PhysicsResult<PluginOutput> {
    if context.resources.missions.len() != 1 {
        return Err(PhysicsError::invalid(
            "fuel sizing",
            "select exactly one mission",
        ));
    }
    let id = context
        .resources
        .missions
        .keys()
        .next()
        .ok_or_else(|| PhysicsError::invalid("fuel sizing", "missing mission"))?;
    let report = crate::size_mission_fuel(
        id,
        &context.resources,
        context.values,
        context.registry,
        crate::FuelSizing {
            minimum_remaining_kg: input(inputs, "minimum_remaining", Unit::Kilogram)?,
            tolerance_kg: input(inputs, "tolerance", Unit::Kilogram)?,
            max_iterations: 64,
        },
    )?;
    result(
        &report,
        &[("required_fuel", report.required_fuel_kg, Unit::Kilogram)],
        None,
        true,
    )
}

fn measurement(context: &AnalysisContext<'_>, volume: bool) -> PhysicsResult<PluginOutput> {
    let (port, unit) = if volume {
        ("volume", Unit::CubicMeter)
    } else {
        ("area", Unit::SquareMeter)
    };
    if context.resources.geometry.is_empty() {
        return Err(PhysicsError::invalid(
            port,
            "select at least one semantic geometry object",
        ));
    }
    let mut contributions = BTreeMap::new();
    for (id, geometry) in &context.resources.geometry {
        let binding = if volume {
            &geometry.volume
        } else {
            &geometry.wetted_area
        };
        let binding = binding
            .as_ref()
            .ok_or_else(|| PhysicsError::invalid(id, format!("explicit {port} is unavailable")))?;
        let value = crate::bindings::scalar(binding, context.values, unit)?;
        contributions.insert(id.clone(), crate::bindings::nonnegative(id, value)?);
    }
    let total = contributions.values().sum();
    result(&contributions, &[(port, total, unit)], None, false)
}

fn geometry(context: &AnalysisContext<'_>) -> PhysicsResult<PluginOutput> {
    let report = evaluate_geometry(&context.resources, context.values)?;
    let clearance = report
        .clearances
        .values()
        .map(|result| result.clearance_m)
        .reduce(f64::min)
        .ok_or_else(|| PhysicsError::invalid("geometry", "select at least one clearance check"))?;
    let margin = report
        .clearances
        .values()
        .map(|result| result.margin_m)
        .reduce(f64::min)
        .ok_or_else(|| PhysicsError::invalid("geometry", "select at least one clearance check"))?;
    result(
        &report,
        &[
            ("minimum_clearance", clearance, Unit::Meter),
            ("minimum_margin", margin, Unit::Meter),
        ],
        None,
        true,
    )
}

fn mass(context: &AnalysisContext<'_>) -> PhysicsResult<PluginOutput> {
    let report = evaluate_mass(&context.resources, context.values)?;
    let cg = report.current.cg_m;
    let failure = report.loadings.iter().find_map(|(id, case)| {
        case.failure
            .as_ref()
            .map(|error| format!("loading {id}: {error}"))
    });
    result(
        &report,
        &[
            ("mass", report.current.total_mass_kg, Unit::Kilogram),
            ("cg_x", cg[0], Unit::Meter),
            ("cg_y", cg[1], Unit::Meter),
            ("cg_z", cg[2], Unit::Meter),
            ("minimum_cg_x", report.minimum_cg_x_m, Unit::Meter),
            ("maximum_cg_x", report.maximum_cg_x_m, Unit::Meter),
        ],
        failure,
        false,
    )
}

fn mission(context: &AnalysisContext<'_>) -> PhysicsResult<PluginOutput> {
    if context.resources.missions.len() != 1 {
        return Err(PhysicsError::invalid(
            "mission",
            "select exactly one mission per analysis",
        ));
    }
    let id = context
        .resources
        .missions
        .keys()
        .next()
        .ok_or_else(|| PhysicsError::invalid("mission", "missing mission"))?;
    let report =
        crate::evaluate_mission_with(id, &context.resources, context.values, context.registry)?;
    let state = &report.final_state.state;
    result(
        &report,
        &[
            ("duration", state.time.value, Unit::Second),
            ("distance", state.distance.value, Unit::Meter),
            ("block_fuel", report.block_fuel_kg, Unit::Kilogram),
            ("reserve_fuel", report.reserve_fuel_kg, Unit::Kilogram),
            (
                "remaining_fuel",
                report.minimum_remaining_fuel_kg(),
                Unit::Kilogram,
            ),
            (
                "final_mass",
                report.final_state.mass.total_mass_kg,
                Unit::Kilogram,
            ),
            ("cost", report.trip_cost_usd, Unit::Dollar),
        ],
        report.scenario_failure(),
        report
            .stages
            .iter()
            .chain(report.branches.values().flat_map(|branch| &branch.stages))
            .any(|stage| stage.approximate),
    )
}

fn operations(context: &AnalysisContext<'_>, hourly: bool) -> PhysicsResult<PluginOutput> {
    let operations = context
        .resources
        .operations
        .as_ref()
        .ok_or_else(|| PhysicsError::invalid("operations", "operating case is not selected"))?;
    let report = evaluate_operations(operations, context.values)?;
    let metric = if hourly {
        (
            "cost_per_hour",
            report.cost_per_block_hour_usd,
            Unit::DollarPerHour,
            "cost per block hour requires a positive normal-trip duration",
        )
    } else {
        (
            "casm",
            report.casm_usd_per_nmi,
            Unit::DollarPerNauticalMile,
            "CASM requires positive seats and distance; use vehicle.trip_cost for hourly accounting",
        )
    };
    let value = metric
        .1
        .ok_or_else(|| PhysicsError::invalid(metric.0, metric.3))?;
    result(
        &report,
        &[
            ("trip_cost", report.trip_cost_usd, Unit::Dollar),
            (metric.0, value, metric.2),
            (
                "network_compatible",
                if report.network_compatible { 1.0 } else { 0.0 },
                Unit::Ratio,
            ),
        ],
        None,
        report.basis.is_none(),
    )
}

fn phase(inputs: &BTreeMap<String, Quantity>) -> PhysicsResult<PluginOutput> {
    let duration = input(inputs, "duration", Unit::Second)?;
    let speed = input(inputs, "speed", Unit::MeterPerSecond)?;
    let flow = input(inputs, "fuel_flow", Unit::KilogramPerSecond)?;
    let cost = input(inputs, "cost_rate", Unit::DollarPerSecond)?;
    result(
        &serde_json::json!({"method":"constant_rates"}),
        &[
            ("duration", duration, Unit::Second),
            ("distance", duration * speed, Unit::Meter),
            ("fuel_burn", duration * flow, Unit::Kilogram),
            ("cost", duration * cost, Unit::Dollar),
        ],
        None,
        true,
    )
}
