use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::result::{AtmosphereState, ConstraintResult, ModelMetadata};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::coefficient_evaluation_at_mach;
use crate::models::atmosphere::scenario_atmosphere;
use crate::models::propulsion::{self, OperatingMode, PropulsionQuery};

/// Design point and requirement values the diagram is drawn against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConstraintTargets {
    /// True airspeed of the cruise and climb curves.
    pub(crate) design_speed_m_s: f64,
    /// Altitude the design speed is flown at.
    pub(crate) design_altitude_m: f64,
    pub(crate) landing_stall_speed_m_s: f64,
    pub(crate) takeoff_distance_m: f64,
}

impl ConstraintTargets {
    #[cfg(test)]
    /// Scenario cruise speed and requirement items, with conceptual defaults when absent.
    pub(crate) fn from_scenario(scenario: &ResolvedScenario) -> Self {
        Self {
            design_speed_m_s: design_speed(scenario),
            design_altitude_m: 0.0,
            landing_stall_speed_m_s: landing_stall_requirement(scenario),
            takeoff_distance_m: takeoff_distance_requirement(scenario),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConstraintAnalyzer {
    scenario: ResolvedScenario,
}

impl ConstraintAnalyzer {
    pub(crate) fn new(scenario: ResolvedScenario) -> Self {
        Self { scenario }
    }

    #[cfg(test)]
    pub(crate) fn analyze(
        &self,
        start_n_m2: f64,
        stop_n_m2: f64,
        count: u32,
    ) -> AexResult<ConstraintResult> {
        self.analyze_with(
            ConstraintTargets::from_scenario(&self.scenario),
            start_n_m2,
            stop_n_m2,
            count,
        )
    }

    pub(crate) fn analyze_with(
        &self,
        targets: ConstraintTargets,
        start_n_m2: f64,
        stop_n_m2: f64,
        count: u32,
    ) -> AexResult<ConstraintResult> {
        let count = count.max(2);
        let wing_loading: Vec<f64> = (0..count)
            .map(|index| {
                start_n_m2 + f64::from(index) * (stop_n_m2 - start_n_m2) / f64::from(count - 1)
            })
            .collect();
        let atmosphere = scenario_atmosphere(&self.scenario).evaluate(0.0)?;
        let design_atmosphere =
            scenario_atmosphere(&self.scenario).evaluate(targets.design_altitude_m)?;
        let clean = &self.scenario.aircraft.aerodynamics.clean;
        let design_speed = targets.design_speed_m_s;
        let clean_evaluation = coefficient_evaluation_at_mach(
            &self.scenario.aircraft,
            "clean",
            design_speed / design_atmosphere.speed_of_sound_m_s,
        )?;
        let takeoff_evaluation =
            coefficient_evaluation_at_mach(&self.scenario.aircraft, "takeoff", 0.0)?;
        let landing_evaluation =
            coefficient_evaluation_at_mach(&self.scenario.aircraft, "landing", 0.0)?;
        let clean_polar = clean_evaluation.coefficients;
        let takeoff_polar = takeoff_evaluation.coefficients;
        let landing_polar = landing_evaluation.coefficients;
        let design_point = DesignPoint {
            dynamic_pressure: 0.5 * design_atmosphere.density_kg_m3 * design_speed.powi(2),
            available_fraction: available_fraction(
                &self.scenario,
                &atmosphere,
                &design_atmosphere,
                design_speed,
            )?,
        };
        let constraints = build_constraints(
            &self.scenario,
            &wing_loading,
            design_point,
            clean_polar.cd0 + clean.additional_cd,
            clean_polar.induced_drag_factor,
            takeoff_polar.cl_max,
            targets,
        );
        let selected_wing_loading = self.scenario.aircraft.mass.maximum_takeoff_mass_kg
            * GRAVITY_M_S2
            / self.scenario.aircraft.wing.area_m2;
        let selected_loading = installed_loading(&self.scenario);
        let stall_limit = 0.5
            * atmosphere.density_kg_m3
            * targets.landing_stall_speed_m_s.powi(2)
            * landing_polar.cl_max;
        let feasible = feasible_mask(&wing_loading, &constraints, stall_limit, selected_loading);
        let selected_index = closest_index(&wing_loading, selected_wing_loading);
        let active = active_constraint(&constraints, selected_index);
        let mut warnings = clean_evaluation.warnings;
        warnings.extend(takeoff_evaluation.warnings);
        warnings.extend(landing_evaluation.warnings);
        warnings.push(Diagnostic::warning(
            WarningCode::ApproximateTakeoffDistance,
            "Takeoff constraint uses a labeled energy approximation without rotation dynamics.",
            "analysis.constraints.takeoff_distance",
        ));
        Ok(ConstraintResult {
            scenario_id: self.scenario.id.clone(),
            x_axis: "W/S [N/m^2]".to_owned(),
            y_axis: match self.scenario.engine {
                EngineProfile::Piston(_) => "P/W [W/kg]",
                EngineProfile::Turbofan(_) => "T/W [N/N]",
            }
            .to_owned(),
            wing_loading_n_m2: wing_loading,
            constraints,
            feasible_region_mask: feasible,
            stall_wing_loading_limit_n_m2: stall_limit,
            selected_wing_loading_n_m2: selected_wing_loading,
            selected_loading,
            active_controlling_constraint: active,
            warnings,
            model: ModelMetadata {
                model_id: "performance.constraint_diagram".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 0,
                validity_status: "approximate".to_owned(),
            },
        })
    }
}

/// Flight condition of the cruise and climb curves.
#[derive(Debug, Clone, Copy)]
struct DesignPoint {
    dynamic_pressure: f64,
    /// Full-throttle power or thrust at the design altitude over the same at
    /// sea level. The installed loading is a sea-level rating, so a
    /// requirement at altitude is divided by this to compare with it.
    available_fraction: f64,
}

/// What is left of the sea-level rating at the design altitude: the piston
/// engine's power lapse, or a turbofan's thrust at the design speed there over
/// its thrust at that speed at sea level.
fn available_fraction(
    scenario: &ResolvedScenario,
    sea_level: &AtmosphereState,
    design: &AtmosphereState,
    speed_m_s: f64,
) -> AexResult<f64> {
    let fraction = match &scenario.engine {
        EngineProfile::Piston(profile) => {
            profile
                .power_lapse
                .fraction(design.density_kg_m3 / sea_level.density_kg_m3)
                .fraction
        }
        EngineProfile::Turbofan(_) => {
            let thrust = |atmosphere: &AtmosphereState| {
                let query = PropulsionQuery {
                    altitude_m: atmosphere.altitude_m,
                    true_airspeed_m_s: speed_m_s,
                    mach: speed_m_s / atmosphere.speed_of_sound_m_s,
                    throttle: 1.0,
                    mode: OperatingMode::Cruise,
                };
                propulsion::evaluate(scenario, atmosphere, query)
                    .map(|state| state.thrust_available_n.unwrap_or(0.0))
            };
            thrust(design)? / thrust(sea_level)?
        }
    };
    if fraction.is_finite() && fraction > 0.0 {
        Ok(fraction)
    } else {
        Err(AexError::analysis(
            "NO_POWER_AT_DESIGN_ALTITUDE",
            format!(
                "the engine has no usable rating at the design altitude {} m",
                design.altitude_m
            ),
        ))
    }
}

fn build_constraints(
    scenario: &ResolvedScenario,
    wing_loading: &[f64],
    design_point: DesignPoint,
    parasite: f64,
    induced: f64,
    takeoff_cl_max: f64,
    targets: ConstraintTargets,
) -> BTreeMap<String, Vec<f64>> {
    let climb_rate = match scenario.engine {
        EngineProfile::Piston(_) => 1.5,
        EngineProfile::Turbofan(_) => 3.0,
    };
    let curve = |rate| {
        wing_loading
            .iter()
            .map(|loading| {
                loading_constraint(
                    scenario,
                    *loading,
                    design_point.dynamic_pressure,
                    parasite,
                    induced,
                    targets.design_speed_m_s,
                    rate,
                ) / design_point.available_fraction
            })
            .collect()
    };
    BTreeMap::from([
        ("cruise_speed".to_owned(), curve(0.0)),
        ("climb_rate".to_owned(), curve(climb_rate)),
        (
            "takeoff_distance_approximate".to_owned(),
            wing_loading
                .iter()
                .map(|loading| {
                    takeoff_constraint(
                        scenario,
                        *loading,
                        takeoff_cl_max,
                        targets.takeoff_distance_m,
                    )
                })
                .collect(),
        ),
    ])
}

fn feasible_mask(
    wing_loading: &[f64],
    constraints: &BTreeMap<String, Vec<f64>>,
    stall_limit: f64,
    selected_loading: f64,
) -> Vec<bool> {
    wing_loading
        .iter()
        .enumerate()
        .map(|(index, loading)| {
            let required = constraints
                .values()
                .filter_map(|curve| curve.get(index))
                .copied()
                .fold(0.0, f64::max);
            *loading <= stall_limit && selected_loading >= required
        })
        .collect()
}

fn loading_constraint(
    scenario: &ResolvedScenario,
    wing_loading: f64,
    dynamic_pressure: f64,
    parasite: f64,
    induced: f64,
    speed: f64,
    climb_rate: f64,
) -> f64 {
    let thrust_to_weight =
        dynamic_pressure * parasite / wing_loading + induced * wing_loading / dynamic_pressure;
    match scenario.engine {
        EngineProfile::Piston(_) => {
            let efficiency = scenario
                .propeller
                .as_ref()
                .map_or(0.75, |item| item.cruise_efficiency);
            (thrust_to_weight * speed + climb_rate) * GRAVITY_M_S2 / efficiency
        }
        EngineProfile::Turbofan(_) => thrust_to_weight + climb_rate / speed,
    }
}

fn takeoff_constraint(
    scenario: &ResolvedScenario,
    wing_loading: f64,
    takeoff_cl_max: f64,
    distance_m: f64,
) -> f64 {
    let lift_off_speed = (2.0 * wing_loading / (1.225 * takeoff_cl_max)).sqrt() * 1.2;
    let thrust_to_weight = lift_off_speed.powi(2) / (2.0 * GRAVITY_M_S2 * distance_m) + 0.04;
    match scenario.engine {
        EngineProfile::Piston(_) => thrust_to_weight * lift_off_speed * GRAVITY_M_S2 / 0.7,
        EngineProfile::Turbofan(_) => thrust_to_weight,
    }
}

#[cfg(test)]
fn design_speed(scenario: &ResolvedScenario) -> f64 {
    scenario
        .mission
        .segments
        .iter()
        .find_map(|segment| segment.true_airspeed_m_s)
        .or(scenario.aircraft.limits.maximum_operating_speed_m_s)
        .unwrap_or(70.0)
}

fn installed_loading(scenario: &ResolvedScenario) -> f64 {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            profile.rated_power_w
                * f64::from(scenario.aircraft.propulsion.engine_count)
                * scenario.aircraft.propulsion.sizing_factor
                / mass
        }
        EngineProfile::Turbofan(profile) => {
            profile.installed_reference_thrust_n(
                scenario.aircraft.propulsion.engine_count,
                scenario.aircraft.propulsion.sizing_factor,
            ) / (mass * GRAVITY_M_S2)
        }
    }
}

#[cfg(test)]
fn landing_stall_requirement(scenario: &ResolvedScenario) -> f64 {
    scenario
        .requirements
        .items
        .iter()
        .find(|item| item.metric == "performance.stall_speed_landing")
        .map_or(30.0, |item| item.required)
}

#[cfg(test)]
fn takeoff_distance_requirement(scenario: &ResolvedScenario) -> f64 {
    scenario
        .requirements
        .items
        .iter()
        .find(|item| item.metric == "performance.takeoff_field_length")
        .map_or(600.0, |item| item.required)
}

fn closest_index(values: &[f64], target: f64) -> usize {
    values
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| (*left - target).abs().total_cmp(&(*right - target).abs()))
        .map_or(0, |(index, _)| index)
}

fn active_constraint(constraints: &BTreeMap<String, Vec<f64>>, selected_index: usize) -> String {
    constraints
        .iter()
        .filter_map(|(name, values)| values.get(selected_index).map(|value| (name, value)))
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map_or_else(|| "none".to_owned(), |(name, _)| name.clone())
}

#[cfg(test)]
mod tests;
