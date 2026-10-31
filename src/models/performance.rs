use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::result::{MissionResult, PerformanceSummary, PointPerformanceResult};
use crate::domain::schema::ResolvedScenario;
use crate::domain::validity::MetricValidity;
use crate::models::aerodynamics::{
    FlightCondition, evaluate as evaluate_aerodynamics, maximum_lift_to_drag_ratio,
    minimum_drag_speed_m_s, minimum_power_speed_m_s, stall_speed_m_s,
};
use crate::models::atmosphere::{Isa1976, scenario_atmosphere};
use crate::models::field_performance::{estimate_landing_distance, estimate_takeoff_distance};
use crate::models::propulsion::{
    OperatingMode, PropulsionQuery, evaluate as evaluate_propulsion, required_fuel_flow,
};

mod ceiling;
mod cruise;
mod solver;
mod trim;
mod validity;

use ceiling::service_ceiling_threshold;
use solver::bracket_roots;
use trim::trim_wing_lift_fraction;
pub(crate) use trim::wing_lift_fraction;
use validity::{
    aggregate as aggregate_validity, from_warnings as validity_from_warnings,
    point_reference_validity, reference_metric_validity, speed_upper_bound,
    summary_metric_validity,
};

const MAXIMUM_SPEED_METRIC: &str = "performance.maximum_level_speed";
const SERVICE_CEILING_METRIC: &str = "performance.service_ceiling";
const ABSOLUTE_CEILING_METRIC: &str = "performance.absolute_ceiling";
const PHYSICAL_SERVICE_CEILING_METRIC: &str = "performance.physical_service_ceiling";
const PHYSICAL_ABSOLUTE_CEILING_METRIC: &str = "performance.physical_absolute_ceiling";
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
            atmosphere: scenario_atmosphere(&scenario),
            scenario,
        }
    }

    pub(crate) fn point(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        configuration: &str,
    ) -> AexResult<PointPerformanceResult> {
        self.point_at_cg(altitude_m, speed_m_s, mass_kg, configuration, None)
    }

    /// Point performance with an optional longitudinal CG: the wing then carries the
    /// trimmed lift (more ahead of the aerodynamic center, less behind it), which
    /// sets stall speed and induced drag; climb still uses the full mass.
    pub(crate) fn point_at_cg(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        configuration: &str,
        center_of_gravity_m: Option<f64>,
    ) -> AexResult<PointPerformanceResult> {
        let trim = center_of_gravity_m
            .map(|cg| trim_wing_lift_fraction(&self.scenario, cg).map(|fraction| (cg, fraction)))
            .transpose()?;
        self.point_trimmed(altitude_m, speed_m_s, mass_kg, configuration, trim)
    }

    /// Point performance with `trim` as the CG station and the wing's share of the
    /// weight that trims there, for callers that hold the stations themselves.
    pub(crate) fn point_trimmed(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        configuration: &str,
        trim: Option<(f64, f64)>,
    ) -> AexResult<PointPerformanceResult> {
        let center_of_gravity_m = trim.map(|(cg, _)| cg);
        let trim = trim.map(|(_, fraction)| fraction);
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let mach = speed_m_s / atmosphere.speed_of_sound_m_s;
        let reference = point_reference_validity(&self.scenario.aircraft, configuration)?;
        let wing_lift_mass_kg = mass_kg * trim.unwrap_or(1.0);
        let stall_speed = stall_speed_m_s(
            &self.scenario.aircraft,
            configuration,
            wing_lift_mass_kg,
            atmosphere.density_kg_m3,
        )?;
        if speed_m_s < stall_speed {
            return Err(AexError::SpeedBelowStall {
                speed_m_s,
                minimum_flyable_speed_m_s: stall_speed,
                altitude_m,
                mass_kg,
                configuration: configuration.to_owned(),
            });
        }
        let aerodynamics = evaluate_aerodynamics(
            &self.scenario.aircraft,
            configuration,
            FlightCondition {
                density_kg_m3: atmosphere.density_kg_m3,
                speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
                true_airspeed_m_s: speed_m_s,
                mass_kg: wing_lift_mass_kg,
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
        let required_flow = required_fuel_flow(
            &self.scenario,
            &aerodynamics,
            altitude_m,
            mach,
            OperatingMode::Cruise,
        )?;
        let (available_power, available_thrust) = available_propulsion(&propulsion, speed_m_s);
        let excess_power = available_power - aerodynamics.power_required_w;
        let excess_thrust = available_thrust - aerodynamics.drag_n;
        let rate_of_climb = excess_power / (mass_kg * GRAVITY_M_S2);
        let mut warnings = aerodynamics.warnings.clone();
        extend_unique_diagnostics(&mut warnings, propulsion.warnings.clone());
        extend_unique_diagnostics(&mut warnings, reference.warnings);
        extend_unique_diagnostics(&mut warnings, required_flow.warnings);
        if center_of_gravity_m.is_some() {
            extend_unique_diagnostics(
                &mut warnings,
                trim::correlated_station_warning(&self.scenario),
            );
        }
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
            fuel_flow_required_kg_s: required_flow.flow_kg_s,
            fuel_flow_max_power_kg_s: propulsion.fuel_flow_kg_s,
            propeller_efficiency: required_flow.propeller_efficiency,
            center_of_gravity_m,
            trim_wing_lift_fraction: trim,
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
        let mass = self
            .scenario
            .mission
            .field
            .takeoff_mass_kg
            .unwrap_or(self.scenario.aircraft.mass.maximum_takeoff_mass_kg);
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
        let physical_service = self.ceiling_solution(mass, threshold)?;
        let physical_absolute = self.ceiling_solution(mass, 0.0)?;
        let service_ceiling = self.published_ceiling(&physical_service);
        let absolute_ceiling = self.published_ceiling(&physical_absolute);
        let cruise = self.cruise_performance(mission)?;
        let reference = reference_metric_validity(&self.scenario.aircraft)?;
        let takeoff_field = estimate_takeoff_distance(&self.scenario)?;
        let landing_field = estimate_landing_distance(&self.scenario)?;
        let (takeoff_distance_m, landing_distance_m) =
            (takeoff_field.distance_m, landing_field.distance_m);
        let mut metric_validity = summary_metric_validity(
            &maximum_speed,
            &service_ceiling,
            &absolute_ceiling,
            &cruise,
            &reference,
            takeoff_field.validity,
            landing_field.validity,
        );
        // The headline aggregate follows the published figures; the physical roots
        // keep their own (possibly extrapolated) validity for analysis.
        let overall_validity = aggregate_validity(metric_validity.values());
        metric_validity.insert(
            PHYSICAL_SERVICE_CEILING_METRIC.to_owned(),
            physical_service.validity.clone(),
        );
        metric_validity.insert(
            PHYSICAL_ABSOLUTE_CEILING_METRIC.to_owned(),
            physical_absolute.validity.clone(),
        );
        let mut ceiling_warnings = Vec::new();
        for (metric, ceiling) in [
            (PHYSICAL_SERVICE_CEILING_METRIC, &physical_service),
            (PHYSICAL_ABSOLUTE_CEILING_METRIC, &physical_absolute),
        ] {
            ceiling_warnings.extend(ceiling.diagnostic(metric));
            ceiling_warnings.extend(self.power_floor_warning(metric, ceiling)?);
        }
        let mut cruise_warnings = cruise.warnings;
        extend_unique_diagnostics(&mut cruise_warnings, ceiling_warnings);
        let warnings = summary_warnings(
            cruise_warnings,
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
            takeoff_field_length_m: Some(takeoff_distance_m),
            landing_field_length_m: Some(landing_distance_m),
            service_ceiling_m: service_ceiling.value,
            absolute_ceiling_m: absolute_ceiling.value,
            ceiling_limited_by: service_ceiling.limited_by.or(absolute_ceiling.limited_by),
            physical_service_ceiling_m: physical_service.value,
            physical_absolute_ceiling_m: physical_absolute.value,
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
