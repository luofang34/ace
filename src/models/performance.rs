use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{FOOT_M, GRAVITY_M_S2};
use crate::domain::result::{MissionResult, PerformanceSummary, PointPerformanceResult};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::models::aerodynamics::{
    FlightCondition, evaluate as evaluate_aerodynamics, maximum_lift_to_drag_ratio,
    minimum_drag_speed_m_s, minimum_power_speed_m_s, stall_speed_m_s,
};
use crate::models::atmosphere::Isa1976;
use crate::models::field_performance::{estimate_landing_distance, estimate_takeoff_distance};
use crate::models::propulsion::{OperatingMode, PropulsionQuery, evaluate as evaluate_propulsion};

mod cruise;
mod solver;
mod validity;

use solver::{bounded_root, bracket_roots};
use validity::{
    aggregate as aggregate_validity, altitude_upper_bound, from_warnings as validity_from_warnings,
    point_reference_validity, reference_metric_validity, speed_upper_bound,
    summary_metric_validity,
};

const MAXIMUM_SPEED_METRIC: &str = "performance.maximum_level_speed";
const SERVICE_CEILING_METRIC: &str = "performance.service_ceiling";
const ABSOLUTE_CEILING_METRIC: &str = "performance.absolute_ceiling";
const ACHIEVED_CRUISE_MACH_METRIC: &str = "performance.achieved_cruise_mach";
const ACHIEVED_CRUISE_TAS_METRIC: &str = "performance.achieved_cruise_true_airspeed";
const MINIMUM_CRUISE_EXCESS_POWER_METRIC: &str = "performance.minimum_cruise_excess_power";
const CRUISE_FEASIBLE_METRIC: &str = "performance.cruise_feasible";

#[derive(Debug, Clone)]
pub(crate) struct PointAnalyzer {
    scenario: ResolvedScenario,
    atmosphere: Isa1976,
}

#[derive(Debug)]
pub(crate) struct ClimbRateEvaluation {
    pub(crate) speed_m_s: f64,
    pub(crate) maximum_rate_m_s: f64,
    pub(crate) warnings: Vec<Diagnostic>,
    validity: MetricValidity,
}

#[derive(Debug)]
struct ExcessPowerEvaluation {
    value_w: f64,
    warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
struct SolvedMetric {
    value: f64,
    validity: MetricValidity,
}

impl PointAnalyzer {
    pub(crate) fn new(scenario: ResolvedScenario) -> Self {
        Self {
            scenario,
            atmosphere: Isa1976::new(0.0),
        }
    }

    pub(crate) fn point(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        configuration: &str,
    ) -> AexResult<PointPerformanceResult> {
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let mach = speed_m_s / atmosphere.speed_of_sound_m_s;
        let reference = point_reference_validity(&self.scenario.aircraft, configuration)?;
        let stall_speed = stall_speed_m_s(
            &self.scenario.aircraft,
            configuration,
            mass_kg,
            atmosphere.density_kg_m3,
        )?;
        if speed_m_s < stall_speed {
            return Err(AexError::analysis(
                "SPEED_BELOW_STALL",
                format!("{speed_m_s:.2} m/s is below stall speed {stall_speed:.2} m/s"),
            ));
        }
        let aerodynamics = evaluate_aerodynamics(
            &self.scenario.aircraft,
            configuration,
            FlightCondition {
                density_kg_m3: atmosphere.density_kg_m3,
                speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
                true_airspeed_m_s: speed_m_s,
                mass_kg,
            },
        )?;
        let propulsion = evaluate_propulsion(
            &self.scenario,
            &atmosphere,
            PropulsionQuery {
                altitude_m,
                true_airspeed_m_s: speed_m_s,
                mach,
                throttle: 1.0,
                mode: OperatingMode::Cruise,
            },
        )?;
        let (available_power, available_thrust) = available_propulsion(&propulsion, speed_m_s);
        let excess_power = available_power - aerodynamics.power_required_w;
        let excess_thrust = available_thrust - aerodynamics.drag_n;
        let rate_of_climb = excess_power / (mass_kg * GRAVITY_M_S2);
        let mut warnings = aerodynamics.warnings.clone();
        extend_unique_diagnostics(&mut warnings, propulsion.warnings.clone());
        extend_unique_diagnostics(&mut warnings, reference.warnings);
        Ok(PointPerformanceResult {
            scenario_id: self.scenario.id.clone(),
            altitude_m,
            true_airspeed_m_s: speed_m_s,
            mach,
            mass_kg,
            configuration: configuration.to_owned(),
            stall_speed_m_s: stall_speed,
            thrust_required_n: aerodynamics.drag_n,
            power_required_w: aerodynamics.power_required_w,
            excess_thrust_n: excess_thrust,
            excess_power_w: excess_power,
            rate_of_climb_m_s: rate_of_climb,
            climb_gradient: excess_thrust / (mass_kg * GRAVITY_M_S2),
            best_glide_speed_m_s: minimum_drag_speed_m_s(
                &self.scenario.aircraft,
                "clean",
                mass_kg,
                atmosphere.density_kg_m3,
            )?,
            maximum_lift_to_drag_ratio: maximum_lift_to_drag_ratio(
                &self.scenario.aircraft,
                "clean",
            )?,
            metric_validity: reference.metric_validity,
            aerodynamics,
            propulsion,
            warnings,
        })
    }

    pub(crate) fn summary(&self, mission: Option<&MissionResult>) -> AexResult<PerformanceSummary> {
        let mass = self.scenario.aircraft.mass.maximum_takeoff_mass_kg;
        let sea_level = self.atmosphere.evaluate(0.0)?;
        let stall_clean = stall_speed_m_s(
            &self.scenario.aircraft,
            "clean",
            mass,
            sea_level.density_kg_m3,
        )?;
        let stall_landing = stall_speed_m_s(
            &self.scenario.aircraft,
            "landing",
            mass,
            sea_level.density_kg_m3,
        )?;
        let best_glide = minimum_drag_speed_m_s(
            &self.scenario.aircraft,
            "clean",
            mass,
            sea_level.density_kg_m3,
        )?;
        let maximum_speed = self.maximum_level_speed_solution(0.0, mass)?;
        let threshold = service_ceiling_threshold(&self.scenario);
        let service_ceiling = self.ceiling_solution(mass, threshold)?;
        let absolute_ceiling = self.ceiling_solution(mass, 0.0)?;
        let cruise = self.cruise_performance(mission)?;
        let reference = reference_metric_validity(&self.scenario.aircraft)?;
        let takeoff_field = estimate_takeoff_distance(&self.scenario)?;
        let landing_field = estimate_landing_distance(&self.scenario)?;
        let metric_validity = summary_metric_validity(
            &maximum_speed,
            &service_ceiling,
            &absolute_ceiling,
            &cruise,
            &reference,
            takeoff_field.validity,
            landing_field.validity,
        );
        let overall_validity = aggregate_validity(metric_validity.values());
        let warnings = summary_warnings(
            cruise.warnings,
            reference.warnings,
            takeoff_field.warnings,
            landing_field.warnings,
        );
        Ok(PerformanceSummary {
            stall_speed_clean_m_s: stall_clean,
            stall_speed_landing_m_s: stall_landing,
            best_glide_speed_m_s: best_glide,
            minimum_power_speed_m_s: minimum_power_speed_m_s(
                &self.scenario.aircraft,
                "clean",
                mass,
                sea_level.density_kg_m3,
            )?,
            maximum_lift_to_drag_ratio: maximum_lift_to_drag_ratio(
                &self.scenario.aircraft,
                "clean",
            )?,
            maximum_level_speed_m_s: maximum_speed.value,
            service_ceiling_m: service_ceiling.value,
            absolute_ceiling_m: absolute_ceiling.value,
            cruise_mach: cruise.declared_mach,
            declared_cruise_mach: cruise.declared_mach,
            declared_cruise_true_airspeed_m_s: cruise.declared_true_airspeed_m_s,
            achieved_cruise_mach: cruise.achieved_mach,
            achieved_cruise_true_airspeed_m_s: cruise.achieved_true_airspeed_m_s,
            minimum_cruise_excess_power_w: cruise.minimum_excess_power_w,
            cruise_feasible: cruise.feasible,
            cruise_conditions: cruise.conditions,
            metric_validity,
            model: crate::domain::result::ModelMetadata {
                model_id: "performance.point_envelope".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 1,
                validity_status: overall_validity.wire_name().to_owned(),
            },
            warnings,
        })
    }

    fn maximum_level_speed_solution(
        &self,
        altitude_m: f64,
        mass_kg: f64,
    ) -> AexResult<SolvedMetric> {
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let stall = stall_speed_m_s(
            &self.scenario.aircraft,
            "clean",
            mass_kg,
            atmosphere.density_kg_m3,
        )?;
        let lower = stall * 1.05;
        let upper = speed_upper_bound(&self.scenario, &atmosphere);
        let roots = bracket_roots(lower, upper.value, 180, |speed| {
            self.excess_power_at(altitude_m, speed, mass_kg, OperatingMode::Cruise, 1.0)
        })?;
        if let Some(root) = roots.last().copied() {
            let validity = if (upper.value - root).abs() <= 1.0e-4 {
                upper.validity()
            } else if (root - lower).abs() <= 1.0e-4 {
                MetricValidity::boundary_limited("performance.maximum_speed_search.minimum")
            } else {
                validity_from_warnings(
                    &self
                        .excess_power_evaluation(
                            altitude_m,
                            root,
                            mass_kg,
                            OperatingMode::Cruise,
                            1.0,
                        )?
                        .warnings,
                )
            };
            return Ok(SolvedMetric {
                value: root,
                validity,
            });
        }
        if self.excess_power_at(altitude_m, upper.value, mass_kg, OperatingMode::Cruise, 1.0)? > 0.0
        {
            return Ok(SolvedMetric {
                value: upper.value,
                validity: upper.validity(),
            });
        }
        Err(AexError::analysis(
            "NO_MAXIMUM_SPEED_INTERSECTION",
            "no high-speed required/available intersection was found",
        ))
    }

    pub(crate) fn maximum_rate_of_climb(
        &self,
        altitude_m: f64,
        mass_kg: f64,
    ) -> AexResult<(f64, f64)> {
        let evaluation = self.maximum_rate_of_climb_with_diagnostics(altitude_m, mass_kg)?;
        Ok((evaluation.speed_m_s, evaluation.maximum_rate_m_s))
    }

    pub(crate) fn maximum_rate_of_climb_with_diagnostics(
        &self,
        altitude_m: f64,
        mass_kg: f64,
    ) -> AexResult<ClimbRateEvaluation> {
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let stall = stall_speed_m_s(
            &self.scenario.aircraft,
            "clean",
            mass_kg,
            atmosphere.density_kg_m3,
        )?;
        let upper = speed_upper_bound(&self.scenario, &atmosphere);
        let mut best_speed = stall * 1.2;
        let mut best_rate = f64::NEG_INFINITY;
        let mut best_index = 0;
        let mut best_validity = MetricValidity::default();
        let mut warnings = Vec::new();
        for index in 0..100 {
            let fraction = f64::from(index) / 99.0;
            let speed = stall * 1.05 + fraction * (upper.value - stall * 1.05);
            let evaluation = self.excess_power_evaluation(
                altitude_m,
                speed,
                mass_kg,
                OperatingMode::Climb,
                1.0,
            )?;
            let validity = validity_from_warnings(&evaluation.warnings);
            let rate = evaluation.value_w / (mass_kg * GRAVITY_M_S2);
            if rate > best_rate {
                best_rate = rate;
                best_speed = speed;
                best_index = index;
                best_validity = validity;
            }
            extend_unique_diagnostics(&mut warnings, evaluation.warnings);
        }
        let validity = if best_index == 0 {
            MetricValidity::boundary_limited("performance.climb_speed_search.minimum")
        } else if best_index == 99 {
            upper.validity()
        } else {
            best_validity
        };
        Ok(ClimbRateEvaluation {
            speed_m_s: best_speed,
            maximum_rate_m_s: best_rate,
            warnings,
            validity,
        })
    }

    fn ceiling_solution(&self, mass_kg: f64, threshold_m_s: f64) -> AexResult<SolvedMetric> {
        let upper = altitude_upper_bound(&self.scenario);
        let function = |altitude| {
            self.maximum_rate_of_climb(altitude, mass_kg)
                .map(|(_, rate)| rate - threshold_m_s)
        };
        let sea_level = function(0.0)?;
        if sea_level <= 0.0 {
            return Ok(SolvedMetric {
                value: 0.0,
                validity: MetricValidity::boundary_limited(
                    "performance.ceiling_search.minimum_altitude",
                ),
            });
        }
        let top = function(upper.value)?;
        if top >= 0.0 {
            return Ok(SolvedMetric {
                value: upper.value,
                validity: upper.validity(),
            });
        }
        let value = bounded_root(0.0, upper.value, 1.0, 80, function)?;
        let evaluation = self.maximum_rate_of_climb_with_diagnostics(value, mass_kg)?;
        Ok(SolvedMetric {
            value,
            validity: evaluation.validity,
        })
    }

    pub(crate) fn excess_power_at(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        mode: OperatingMode,
        throttle: f64,
    ) -> AexResult<f64> {
        Ok(self
            .excess_power_evaluation(altitude_m, speed_m_s, mass_kg, mode, throttle)?
            .value_w)
    }

    pub(crate) fn excess_power_with_diagnostics(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        mode: OperatingMode,
        throttle: f64,
    ) -> AexResult<(f64, Vec<Diagnostic>)> {
        let evaluation =
            self.excess_power_evaluation(altitude_m, speed_m_s, mass_kg, mode, throttle)?;
        Ok((evaluation.value_w, evaluation.warnings))
    }

    fn excess_power_evaluation(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        mode: OperatingMode,
        throttle: f64,
    ) -> AexResult<ExcessPowerEvaluation> {
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let mach = speed_m_s / atmosphere.speed_of_sound_m_s;
        let aero = evaluate_aerodynamics(
            &self.scenario.aircraft,
            "clean",
            FlightCondition {
                density_kg_m3: atmosphere.density_kg_m3,
                speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
                true_airspeed_m_s: speed_m_s,
                mass_kg,
            },
        )?;
        let propulsion = evaluate_propulsion(
            &self.scenario,
            &atmosphere,
            PropulsionQuery {
                altitude_m,
                true_airspeed_m_s: speed_m_s,
                mach,
                throttle,
                mode,
            },
        )?;
        let available = propulsion
            .propulsive_power_available_w
            .or_else(|| {
                propulsion
                    .thrust_available_n
                    .map(|thrust| thrust * speed_m_s)
            })
            .ok_or_else(|| {
                AexError::analysis("MISSING_PROPULSION_CAPABILITY", "no power or thrust")
            })?;
        let mut warnings = aero.warnings;
        extend_unique_diagnostics(&mut warnings, propulsion.warnings);
        Ok(ExcessPowerEvaluation {
            value_w: available - aero.power_required_w,
            warnings,
        })
    }
}

fn available_propulsion(
    propulsion: &crate::domain::result::PropulsionState,
    speed_m_s: f64,
) -> (f64, f64) {
    let power = propulsion
        .propulsive_power_available_w
        .unwrap_or_else(|| propulsion.thrust_available_n.unwrap_or(0.0) * speed_m_s);
    let thrust = propulsion
        .thrust_available_n
        .unwrap_or_else(|| power / speed_m_s);
    (power, thrust)
}

fn service_ceiling_threshold(scenario: &ResolvedScenario) -> f64 {
    match scenario.engine {
        EngineProfile::Piston(_) => 100.0 * FOOT_M / 60.0,
        EngineProfile::Turbofan(_) => 500.0 * FOOT_M / 60.0,
    }
}

fn summary_warnings(
    mut warnings: Vec<Diagnostic>,
    reference: Vec<Diagnostic>,
    takeoff_field: Vec<Diagnostic>,
    landing_field: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    extend_unique_diagnostics(&mut warnings, reference);
    extend_unique_diagnostics(&mut warnings, takeoff_field);
    extend_unique_diagnostics(&mut warnings, landing_field);
    warnings.push(Diagnostic::limitation(
        "Ceilings use quasi-steady maximum excess-power sampling.",
    ));
    warnings
}

fn extend_unique_diagnostics(target: &mut Vec<Diagnostic>, diagnostics: Vec<Diagnostic>) {
    for diagnostic in diagnostics {
        if !target
            .iter()
            .any(|existing| existing.code == diagnostic.code && existing.path == diagnostic.path)
        {
            target.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests;
