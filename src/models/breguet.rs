use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::schema::{EngineProfile, ResolvedScenario, SegmentKind};
use crate::domain::warning::WarningCode;
use crate::models::atmosphere::Isa1976;
use crate::models::propulsion::{OperatingMode, ThrustFuelBasis, fuel_flow_for_required_thrust};

#[derive(Debug, Clone)]
pub(crate) struct BreguetEstimate {
    pub(crate) range_m: f64,
    pub(crate) endurance_s: f64,
    pub(crate) assumptions: Vec<String>,
    pub(crate) warnings: Vec<Diagnostic>,
}

pub(crate) fn estimate(
    scenario: &ResolvedScenario,
    lift_to_drag_ratio: f64,
    fuel_burn_kg: f64,
) -> AexResult<BreguetEstimate> {
    let initial_mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let usable_burn = fuel_burn_kg.clamp(0.0, initial_mass * 0.95);
    let final_mass = initial_mass - usable_burn;
    let (altitude_m, speed, mach) = cruise_condition(scenario)?;
    let mass_ratio = (initial_mass / final_mass).ln();
    let (range_m, method, propulsion_warnings) = match &scenario.engine {
        EngineProfile::Piston(engine) => {
            let propeller_efficiency = scenario
                .propeller
                .as_ref()
                .map_or(0.78, |propeller| propeller.cruise_efficiency);
            let bsfc_kg_j = engine.bsfc_cruise_kg_kwh / 3_600_000.0;
            (
                propeller_efficiency / (GRAVITY_M_S2 * bsfc_kg_j) * lift_to_drag_ratio * mass_ratio,
                "propeller Breguet range with cruise BSFC",
                Vec::new(),
            )
        }
        EngineProfile::Turbofan(engine) => {
            let fuel = fuel_flow_for_required_thrust(
                engine,
                altitude_m,
                mach,
                OperatingMode::Cruise,
                1.0,
            )?;
            (
                speed / (GRAVITY_M_S2 * fuel.flow_kg_s) * lift_to_drag_ratio * mass_ratio,
                match fuel.basis {
                    ThrustFuelBasis::Tsfc => "jet Breguet range with cruise TSFC",
                    ThrustFuelBasis::SpecificImpulse => {
                        "jet Breguet range with cruise specific impulse"
                    }
                },
                fuel.warnings,
            )
        }
    };
    let mut warnings = propulsion_warnings;
    if usable_burn <= f64::EPSILON {
        warnings.push(Diagnostic::warning(
            WarningCode::ZeroBreguetFuelBurn,
            "The Breguet estimate is zero because the simulated mission burned no fuel.",
            "mission.total_fuel_burn",
        ));
    }
    Ok(BreguetEstimate {
        range_m,
        endurance_s: range_m / speed,
        assumptions: vec![
            method.to_owned(),
            "constant cruise speed and lift-to-drag ratio".to_owned(),
            "takeoff mass is the initial Breguet mass".to_owned(),
        ],
        warnings,
    })
}

fn cruise_condition(scenario: &ResolvedScenario) -> AexResult<(f64, f64, f64)> {
    let segment = scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise);
    let altitude = segment.and_then(|item| item.altitude_m).unwrap_or(0.0);
    if let Some(speed) = segment.and_then(|item| item.true_airspeed_m_s) {
        let atmosphere = Isa1976::new(0.0).evaluate(altitude)?;
        return Ok((altitude, speed, speed / atmosphere.speed_of_sound_m_s));
    }
    let mach = segment.and_then(|item| item.mach).unwrap_or(0.3);
    let atmosphere = Isa1976::new(0.0).evaluate(altitude)?;
    Ok((altitude, mach * atmosphere.speed_of_sound_m_s, mach))
}

#[cfg(test)]
mod tests;
