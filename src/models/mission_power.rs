use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MissionPowerPoint, MissionPowerScreen, MissionResult, ResultProvenance,
};
use crate::domain::schema::{EngineProfile, ResolvedScenario, SegmentKind};
use crate::models::mission::{representative_speed, segment_engine_off};
use crate::models::performance::PointAnalyzer;
use crate::models::propulsion::OperatingMode;

pub(crate) fn evaluate(
    scenario: &ResolvedScenario,
    mission: &MissionResult,
) -> AexResult<MissionPowerScreen> {
    let analyzer = PointAnalyzer::new(scenario.clone());
    let mut points = Vec::new();
    for result in &mission.segments {
        let Some(segment) = scenario
            .mission
            .segments
            .iter()
            .find(|item| item.id == result.segment_id)
        else {
            continue;
        };
        if segment_engine_off(segment) {
            continue;
        }
        let Some(mode) = operating_mode(segment.kind) else {
            continue;
        };
        let altitude = segment_altitude(segment.kind, result);
        let speed = result
            .operating_speed_m_s
            .map_or_else(|| representative_speed(segment, scenario, altitude), Ok)?;
        let mass = 0.5 * (result.start_mass_kg + result.end_mass_kg);
        let throttle = segment
            .power_fraction
            .or(segment.thrust_fraction)
            .unwrap_or(1.0);
        let excess = analyzer.excess_power_at(altitude, speed, mass, mode, throttle)?;
        let reserve = required_reserve_power_w(scenario, speed);
        points.push(MissionPowerPoint {
            segment_id: segment.id.clone(),
            altitude: QuantityOutput::si(altitude, "m"),
            true_airspeed: QuantityOutput::si(speed, "m/s"),
            mass: QuantityOutput::si(mass, "kg"),
            throttle,
            excess_power: QuantityOutput::si(excess, "W"),
            required_reserve: QuantityOutput::si(reserve, "W"),
            passed: excess >= reserve,
        });
    }
    let failed_constraints = points
        .iter()
        .filter(|point| !point.passed)
        .map(|point| format!("mission_power.{}", point.segment_id))
        .collect::<Vec<_>>();
    let minimum = points
        .iter()
        .map(|point| point.excess_power.value)
        .reduce(f64::min)
        .unwrap_or(0.0);
    let minimum_reserve_margin = points
        .iter()
        .map(|point| point.excess_power.value - point.required_reserve.value)
        .reduce(f64::min)
        .unwrap_or(0.0);
    Ok(MissionPowerScreen {
        passed: failed_constraints.is_empty(),
        minimum_excess_power: QuantityOutput::si(minimum, "W"),
        minimum_reserve_margin: QuantityOutput::si(minimum_reserve_margin, "W"),
        required_reserve_fraction: 0.03,
        points,
        failed_constraints,
        provenance: provenance(),
    })
}

fn required_reserve_power_w(scenario: &ResolvedScenario, speed_m_s: f64) -> f64 {
    let count = f64::from(scenario.aircraft.propulsion.engine_count);
    let sizing = scenario.aircraft.propulsion.sizing_factor;
    match &scenario.engine {
        EngineProfile::Piston(profile) => 0.03 * profile.rated_power_w * count * sizing,
        EngineProfile::Turbofan(profile) => {
            0.03 * profile.sea_level_static_thrust_n * count * sizing * speed_m_s
        }
    }
}

fn operating_mode(kind: SegmentKind) -> Option<OperatingMode> {
    match kind {
        SegmentKind::Climb => Some(OperatingMode::Climb),
        SegmentKind::Cruise => Some(OperatingMode::Cruise),
        SegmentKind::Loiter | SegmentKind::Reserve => Some(OperatingMode::Economy),
        _ => None,
    }
}

fn segment_altitude(
    kind: SegmentKind,
    result: &crate::domain::result::MissionSegmentResult,
) -> f64 {
    if kind == SegmentKind::Climb {
        0.5 * (result.start_altitude_m + result.end_altitude_m)
    } else {
        result.end_altitude_m
    }
}

fn provenance() -> ResultProvenance {
    ResultProvenance {
        method: "segment operating-point required-versus-available power check".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec![
            "segment midpoint mass".to_owned(),
            "declared segment power or thrust fraction".to_owned(),
            "3% installed reference power reserve".to_owned(),
            "quasi-steady unaccelerated flight".to_owned(),
        ],
        validity_range: vec!["climb, cruise, loiter, and reserve segments".to_owned()],
        validity_domains: Vec::new(),
        units: BTreeMap::from([
            ("altitude".to_owned(), "m".to_owned()),
            ("speed".to_owned(), "m/s".to_owned()),
            ("mass".to_owned(), "kg".to_owned()),
            ("power_margin".to_owned(), "W".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "Power margins do not include transient, cooling, mixture, or propeller-map limits.",
        )],
    }
}

#[cfg(test)]
mod tests;
