use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MassPropertiesAnalysis, MassPropertiesMetricSnapshot, MassPropertiesState, MissionResult,
    ResultProvenance,
};
use crate::domain::schema::ResolvedScenario;

const MODEL_ID: &str = "stability.native_mass_properties";
const MODEL_VERSION: u32 = 1;
const MINIMUM_STABLE_MARGIN: f64 = 0.0;

pub(crate) fn evaluate(
    scenario: &ResolvedScenario,
    mission: &MissionResult,
) -> AexResult<MassPropertiesAnalysis> {
    let empty_moment = operating_empty_moment(scenario)?;
    let neutral_point = neutral_point_m(scenario);
    let mean_chord = scenario.aircraft.wing.area_m2 / scenario.aircraft.wing.span_m;
    let mut states = mission_states(scenario, mission, empty_moment, neutral_point, mean_chord)?;
    let snapshot = metric_snapshot(&states)?;
    let failed_constraints = failed_constraints(&snapshot);
    Ok(MassPropertiesAnalysis {
        statement: scenario.aircraft.mass.statement.clone(),
        closure_error: QuantityOutput::si(scenario.aircraft.mass.statement.closure_error_kg, "kg"),
        minimum_center_of_gravity: QuantityOutput::si(snapshot.minimum_center_of_gravity_m, "m"),
        maximum_center_of_gravity: QuantityOutput::si(snapshot.maximum_center_of_gravity_m, "m"),
        neutral_point: neutral_point.map(|value| QuantityOutput::si(value, "m")),
        minimum_static_margin: snapshot.minimum_static_margin,
        maximum_static_margin: static_margin_bounds(&states).map(|(_, maximum)| maximum),
        stability_supported: neutral_point.is_some(),
        failed_constraints,
        provenance: provenance(neutral_point.is_some()),
        states: std::mem::take(&mut states),
    })
}

pub(crate) fn metric_snapshot(
    states: &[MassPropertiesState],
) -> AexResult<MassPropertiesMetricSnapshot> {
    let minimum_center_of_gravity_m = states
        .iter()
        .map(|state| state.center_of_gravity.value)
        .reduce(f64::min)
        .ok_or_else(|| missing_state_error("minimum"))?;
    let maximum_center_of_gravity_m = states
        .iter()
        .map(|state| state.center_of_gravity.value)
        .reduce(f64::max)
        .ok_or_else(|| missing_state_error("maximum"))?;
    Ok(MassPropertiesMetricSnapshot {
        minimum_center_of_gravity_m,
        maximum_center_of_gravity_m,
        minimum_static_margin: static_margin_bounds(states).map(|(minimum, _)| minimum),
    })
}

fn operating_empty_moment(scenario: &ResolvedScenario) -> AexResult<f64> {
    let statement = &scenario.aircraft.mass.statement;
    let mass = statement
        .components
        .iter()
        .map(|component| component.mass.value)
        .sum::<f64>();
    if (mass - scenario.aircraft.mass.operating_empty_mass_kg).abs() > 1.0e-6 {
        return Err(AexError::validation(
            "MASS_STATEMENT_NOT_CLOSED",
            "aircraft.mass.components",
            format!(
                "component mass {mass:.6} kg does not close to OEW {:.6} kg",
                scenario.aircraft.mass.operating_empty_mass_kg
            ),
        ));
    }
    Ok(statement
        .components
        .iter()
        .map(|component| component.mass.value * component.station.value)
        .sum())
}

fn mission_states(
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    empty_moment: f64,
    neutral_point: Option<f64>,
    mean_chord: f64,
) -> AexResult<Vec<MassPropertiesState>> {
    let empty_mass = scenario.aircraft.mass.operating_empty_mass_kg;
    let mut payload = scenario.mission.payload_mass_kg;
    let initial_fuel = mission.initial_takeoff_mass_kg - empty_mass - payload;
    let mut states = vec![state(
        "mission.start",
        mission.initial_takeoff_mass_kg,
        initial_fuel,
        payload,
        scenario,
        empty_moment,
        neutral_point,
        mean_chord,
    )?];
    for segment in &mission.segments {
        payload = (payload - segment.payload_removed_kg).max(0.0);
        let fuel = (segment.end_mass_kg - empty_mass - payload).max(0.0);
        states.push(state(
            &format!("mission.segments.{}.end", segment.segment_id),
            segment.end_mass_kg,
            fuel,
            payload,
            scenario,
            empty_moment,
            neutral_point,
            mean_chord,
        )?);
    }
    Ok(states)
}

#[allow(clippy::too_many_arguments)]
fn state(
    id: &str,
    total_mass: f64,
    fuel_mass: f64,
    payload_mass: f64,
    scenario: &ResolvedScenario,
    empty_moment: f64,
    neutral_point: Option<f64>,
    mean_chord: f64,
) -> AexResult<MassPropertiesState> {
    if total_mass <= 0.0 || fuel_mass < 0.0 || payload_mass < 0.0 {
        return Err(AexError::validation(
            "INVALID_MISSION_MASS_STATE",
            id,
            "mission mass, fuel, and payload must remain non-negative",
        ));
    }
    let statement = &scenario.aircraft.mass.statement;
    let moment = empty_moment
        + fuel_mass * statement.fuel_station.value
        + payload_mass * statement.payload_station.value;
    let center_of_gravity = moment / total_mass;
    Ok(MassPropertiesState {
        id: id.to_owned(),
        total_mass: QuantityOutput::si(total_mass, "kg"),
        fuel_mass: QuantityOutput::si(fuel_mass, "kg"),
        payload_mass: QuantityOutput::si(payload_mass, "kg"),
        center_of_gravity: QuantityOutput::si(center_of_gravity, "m"),
        static_margin: neutral_point.map(|point| (point - center_of_gravity) / mean_chord),
    })
}

fn neutral_point_m(scenario: &ResolvedScenario) -> Option<f64> {
    if !scenario
        .aircraft
        .topology
        .has_component_kind("horizontal_tail")
    {
        return None;
    }
    let geometry = &scenario.aircraft.geometry;
    let fuselage = geometry.fuselage.as_ref()?;
    let tail = geometry.horizontal_tail.as_ref()?;
    let wing = &scenario.aircraft.wing;
    let mean_chord = wing.area_m2 / wing.span_m;
    let wing_leading_edge = if scenario.aircraft.category.contains("transport") {
        0.42 * fuselage.length.value
    } else {
        0.34 * fuselage.length.value
    };
    let tail_volume = tail.area.value * tail.arm.value / (wing.area_m2 * mean_chord);
    Some(wing_leading_edge + (0.25 + 0.60 * tail_volume) * mean_chord)
}

fn static_margin_bounds(states: &[MassPropertiesState]) -> Option<(f64, f64)> {
    let mut values = states.iter().filter_map(|state| state.static_margin);
    let first = values.next()?;
    Some(values.fold((first, first), |(minimum, maximum), value| {
        (minimum.min(value), maximum.max(value))
    }))
}

fn failed_constraints(snapshot: &MassPropertiesMetricSnapshot) -> Vec<String> {
    if snapshot
        .minimum_static_margin
        .is_some_and(|margin| margin <= MINIMUM_STABLE_MARGIN)
    {
        vec!["stability.static_margin".to_owned()]
    } else {
        Vec::new()
    }
}

fn provenance(stability_supported: bool) -> ResultProvenance {
    let validity = if stability_supported {
        "conventional aircraft with a resolved horizontal tail"
    } else {
        "mass properties only; tailless neutral-point stability is unsupported"
    };
    ResultProvenance {
        method: format!("{MODEL_ID}.v{MODEL_VERSION}"),
        backend: "native".to_owned(),
        assumptions: vec![
            "component masses close exactly to declared operating empty mass".to_owned(),
            "fuel and payload use their resolved longitudinal centroid stations".to_owned(),
            "neutral point uses a conceptual horizontal-tail volume correlation".to_owned(),
        ],
        validity_range: vec![validity.to_owned()],
        validity_domains: Vec::new(),
        units: BTreeMap::from([
            ("mass".to_owned(), "kg".to_owned()),
            ("station".to_owned(), "m".to_owned()),
            ("static_margin".to_owned(), "1".to_owned()),
        ]),
        warnings: if stability_supported {
            Vec::new()
        } else {
            vec![Diagnostic::limitation(
                "Native neutral-point stability is unavailable for tailless topology.",
            )]
        },
    }
}

fn missing_state_error(bound: &str) -> AexError {
    AexError::validation(
        "MISSING_MASS_PROPERTIES_STATE",
        "analysis.mass_properties.states",
        format!("cannot compute {bound} CG without a mission mass state"),
    )
}

#[cfg(test)]
mod tests;
