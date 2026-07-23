use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{FOOT_M, GRAVITY_M_S2};
use crate::domain::result::{PerformanceSummary, PointPerformanceResult};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::models::aerodynamics::{
    FlightCondition, evaluate as evaluate_aerodynamics, maximum_lift_to_drag_ratio,
    minimum_drag_speed_m_s, minimum_power_speed_m_s, stall_speed_m_s,
};
use crate::models::atmosphere::Isa1976;
use crate::models::propulsion::{OperatingMode, PropulsionQuery, evaluate as evaluate_propulsion};

#[derive(Debug, Clone)]
pub(crate) struct PointAnalyzer {
    scenario: ResolvedScenario,
    atmosphere: Isa1976,
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
                mode: OperatingMode::Climb,
            },
        )?;
        let available_power = propulsion
            .propulsive_power_available_w
            .unwrap_or_else(|| propulsion.thrust_available_n.unwrap_or(0.0) * speed_m_s);
        let available_thrust = propulsion
            .thrust_available_n
            .unwrap_or_else(|| available_power / speed_m_s);
        let excess_power = available_power - aerodynamics.power_required_w;
        let excess_thrust = available_thrust - aerodynamics.drag_n;
        let rate_of_climb = excess_power / (mass_kg * GRAVITY_M_S2);
        let mut warnings = aerodynamics.warnings.clone();
        warnings.extend(propulsion.warnings.clone());
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
            aerodynamics,
            propulsion,
            warnings,
        })
    }

    pub(crate) fn summary(&self) -> AexResult<PerformanceSummary> {
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
        let maximum_speed = self.maximum_level_speed(0.0, mass)?;
        let threshold = match self.scenario.engine {
            EngineProfile::Piston(_) => 100.0 * FOOT_M / 60.0,
            EngineProfile::Turbofan(_) => 500.0 * FOOT_M / 60.0,
        };
        let service_ceiling = self.ceiling(mass, threshold)?;
        let absolute_ceiling = self.ceiling(mass, 0.0)?;
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
            maximum_level_speed_m_s: maximum_speed,
            service_ceiling_m: service_ceiling,
            absolute_ceiling_m: absolute_ceiling,
            cruise_mach: mission_cruise_mach(&self.scenario),
            model: crate::domain::result::ModelMetadata {
                model_id: "performance.point_envelope".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 1,
                validity_status: "valid".to_owned(),
            },
            warnings: vec![Diagnostic::limitation(
                "Ceilings use quasi-steady maximum excess-power sampling.",
            )],
        })
    }

    pub(crate) fn maximum_level_speed(&self, altitude_m: f64, mass_kg: f64) -> AexResult<f64> {
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let stall = stall_speed_m_s(
            &self.scenario.aircraft,
            "clean",
            mass_kg,
            atmosphere.density_kg_m3,
        )?;
        let upper = speed_upper_bound(&self.scenario, &atmosphere);
        let roots = bracket_roots(stall * 1.05, upper, 180, |speed| {
            self.excess_power(altitude_m, speed, mass_kg, OperatingMode::Cruise)
        })?;
        if let Some(root) = roots.last().copied() {
            return Ok(root);
        }
        if self.excess_power(altitude_m, upper, mass_kg, OperatingMode::Cruise)? > 0.0 {
            return Ok(upper);
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
        for index in 0..100 {
            let fraction = f64::from(index) / 99.0;
            let speed = stall * 1.05 + fraction * (upper - stall * 1.05);
            let rate = self.excess_power(altitude_m, speed, mass_kg, OperatingMode::Climb)?
                / (mass_kg * GRAVITY_M_S2);
            if rate > best_rate {
                best_rate = rate;
                best_speed = speed;
            }
        }
        Ok((best_speed, best_rate))
    }

    pub(crate) fn ceiling(&self, mass_kg: f64, threshold_m_s: f64) -> AexResult<f64> {
        let maximum_altitude = self
            .scenario
            .aircraft
            .limits
            .maximum_operating_altitude_m
            .unwrap_or(19_900.0)
            .min(19_900.0);
        let function = |altitude| {
            self.maximum_rate_of_climb(altitude, mass_kg)
                .map(|(_, rate)| rate - threshold_m_s)
        };
        let sea_level = function(0.0)?;
        if sea_level <= 0.0 {
            return Ok(0.0);
        }
        let top = function(maximum_altitude)?;
        if top > 0.0 {
            return Ok(maximum_altitude);
        }
        bounded_root(0.0, maximum_altitude, 1.0, 80, function)
    }

    fn excess_power(
        &self,
        altitude_m: f64,
        speed_m_s: f64,
        mass_kg: f64,
        mode: OperatingMode,
    ) -> AexResult<f64> {
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
                throttle: 1.0,
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
        Ok(available - aero.power_required_w)
    }
}

fn speed_upper_bound(
    scenario: &ResolvedScenario,
    atmosphere: &crate::domain::result::AtmosphereState,
) -> f64 {
    let mach_limit = scenario
        .aircraft
        .limits
        .maximum_operating_mach
        .unwrap_or(0.95)
        * atmosphere.speed_of_sound_m_s;
    scenario
        .aircraft
        .limits
        .maximum_operating_speed_m_s
        .unwrap_or(mach_limit)
        .min(mach_limit)
}

fn bracket_roots<F>(start: f64, stop: f64, count: u32, function: F) -> AexResult<Vec<f64>>
where
    F: Fn(f64) -> AexResult<f64>,
{
    let mut roots = Vec::new();
    let mut left = start;
    let mut left_value = function(left)?;
    for index in 1..=count {
        let right = start + f64::from(index) * (stop - start) / f64::from(count);
        let right_value = function(right)?;
        if left_value * right_value <= 0.0 {
            roots.push(bounded_root(left, right, 1.0e-5, 80, &function)?);
        }
        left = right;
        left_value = right_value;
    }
    Ok(roots)
}

fn bounded_root<F>(
    mut lower: f64,
    mut upper: f64,
    tolerance: f64,
    iterations: u32,
    function: F,
) -> AexResult<f64>
where
    F: Fn(f64) -> AexResult<f64>,
{
    let mut lower_value = function(lower)?;
    let upper_value = function(upper)?;
    if lower_value * upper_value > 0.0 {
        return Err(AexError::analysis(
            "ROOT_NOT_BRACKETED",
            format!("function has the same sign at {lower} and {upper}"),
        ));
    }
    for _ in 0..iterations {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = function(midpoint)?;
        if (upper - lower).abs() <= tolerance {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(AexError::analysis(
        "ROOT_NON_CONVERGENCE",
        format!("bounded solver did not converge in {iterations} iterations"),
    ))
}

fn mission_cruise_mach(scenario: &ResolvedScenario) -> Option<f64> {
    scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == crate::domain::schema::SegmentKind::Cruise)
        .and_then(|segment| segment.mach)
}

#[cfg(test)]
mod tests;
