use super::solver::bounded_root;
use super::validity::{
    altitude_upper_bound, from_warnings as validity_from_warnings, speed_upper_bound,
};
use super::{ClimbRateEvaluation, PointAnalyzer, SolvedMetric, extend_unique_diagnostics};
use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::{FOOT_M, GRAVITY_M_S2};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::models::aerodynamics::stall_speed_m_s;
use crate::models::propulsion::OperatingMode;

impl PointAnalyzer {
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

    pub(super) fn ceiling_solution(
        &self,
        mass_kg: f64,
        threshold_m_s: f64,
    ) -> AexResult<SolvedMetric> {
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
}

pub(super) fn service_ceiling_threshold(scenario: &ResolvedScenario) -> f64 {
    match scenario.engine {
        EngineProfile::Piston(_) => 100.0 * FOOT_M / 60.0,
        EngineProfile::Turbofan(_) => 500.0 * FOOT_M / 60.0,
    }
}
