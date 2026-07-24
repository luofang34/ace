use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::result::{ConstraintResult, ModelMetadata};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::models::atmosphere::Isa1976;

#[derive(Debug, Clone)]
pub(crate) struct ConstraintAnalyzer {
    scenario: ResolvedScenario,
}

impl ConstraintAnalyzer {
    pub(crate) fn new(scenario: ResolvedScenario) -> Self {
        Self { scenario }
    }

    pub(crate) fn analyze(
        &self,
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
        let atmosphere = Isa1976::new(0.0).evaluate(0.0)?;
        let clean = &self.scenario.aircraft.aerodynamics.clean;
        let parasite = clean.cd0 + clean.additional_cd;
        let induced = 1.0
            / (std::f64::consts::PI
                * clean.oswald_efficiency
                * self.scenario.aircraft.wing.aspect_ratio);
        let design_speed = design_speed(&self.scenario);
        let dynamic_pressure = 0.5 * atmosphere.density_kg_m3 * design_speed.powi(2);
        let constraints = build_constraints(
            &self.scenario,
            &wing_loading,
            dynamic_pressure,
            parasite,
            induced,
            design_speed,
        );
        let selected_wing_loading = self.scenario.aircraft.mass.maximum_takeoff_mass_kg
            * GRAVITY_M_S2
            / self.scenario.aircraft.wing.area_m2;
        let selected_loading = installed_loading(&self.scenario);
        let stall_limit = 0.5
            * atmosphere.density_kg_m3
            * landing_stall_requirement(&self.scenario).powi(2)
            * self.scenario.aircraft.aerodynamics.landing.cl_max;
        let feasible = feasible_mask(&wing_loading, &constraints, stall_limit, selected_loading);
        let selected_index = closest_index(&wing_loading, selected_wing_loading);
        let active = active_constraint(&constraints, selected_index);
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
            selected_wing_loading_n_m2: selected_wing_loading,
            selected_loading,
            active_controlling_constraint: active,
            warnings: vec![Diagnostic::warning(
                "APPROXIMATE_TAKEOFF_DISTANCE",
                "Takeoff constraint uses a labeled energy approximation without rotation dynamics.",
                "analysis.constraints.takeoff_distance",
            )],
            model: ModelMetadata {
                model_id: "performance.constraint_diagram".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 0,
                validity_status: "approximate".to_owned(),
            },
        })
    }
}

fn build_constraints(
    scenario: &ResolvedScenario,
    wing_loading: &[f64],
    dynamic_pressure: f64,
    parasite: f64,
    induced: f64,
    design_speed: f64,
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
                    dynamic_pressure,
                    parasite,
                    induced,
                    design_speed,
                    rate,
                )
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
                .map(|loading| takeoff_constraint(scenario, *loading))
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

fn takeoff_constraint(scenario: &ResolvedScenario, wing_loading: f64) -> f64 {
    let distance_m = takeoff_distance_requirement(scenario);
    let lift_off_speed =
        (2.0 * wing_loading / (1.225 * scenario.aircraft.aerodynamics.takeoff.cl_max)).sqrt() * 1.2;
    let thrust_to_weight = lift_off_speed.powi(2) / (2.0 * GRAVITY_M_S2 * distance_m) + 0.04;
    match scenario.engine {
        EngineProfile::Piston(_) => thrust_to_weight * lift_off_speed * GRAVITY_M_S2 / 0.7,
        EngineProfile::Turbofan(_) => thrust_to_weight,
    }
}

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
            profile.sea_level_static_thrust_n
                * f64::from(scenario.aircraft.propulsion.engine_count)
                * scenario.aircraft.propulsion.sizing_factor
                / (mass * GRAVITY_M_S2)
        }
    }
}

fn landing_stall_requirement(scenario: &ResolvedScenario) -> f64 {
    scenario
        .requirements
        .items
        .iter()
        .find(|item| item.metric == "performance.stall_speed_landing")
        .map_or(30.0, |item| item.required)
}

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
