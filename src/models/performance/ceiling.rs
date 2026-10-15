use super::solver::bounded_root;
use super::validity::{
    altitude_upper_bound, from_warnings as validity_from_warnings, speed_upper_bound,
};
use super::{ClimbRateEvaluation, PointAnalyzer, extend_unique_diagnostics};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::{FOOT_M, GRAVITY_M_S2};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::stall_speed_m_s;
use crate::models::propulsion::{OperatingMode, shaft_power_floored};

/// A solved ceiling, or the reason no ceiling exists inside the searchable range.
#[derive(Debug)]
pub(super) struct CeilingMetric {
    pub(super) value: Option<f64>,
    pub(super) validity: MetricValidity,
    pub(super) reason: Option<String>,
}

impl CeilingMetric {
    fn unavailable(reason: String) -> Self {
        Self {
            value: None,
            validity: MetricValidity::unsupported(),
            reason: Some(reason),
        }
    }

    pub(super) fn diagnostic(&self, metric: &str) -> Option<Diagnostic> {
        self.reason.as_ref().map(|reason| {
            Diagnostic::warning_with_context(
                WarningCode::CeilingUnavailable,
                format!("{metric} is unavailable: {reason}"),
                metric,
                serde_json::json!({"metric": metric}),
            )
        })
    }
}

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

    /// Solves for the altitude where the best climb rate meets `threshold_m_s`.
    /// A declared airframe or engine altitude limit clamps the result and is named
    /// as its boundary; when no root lies between sea level and the atmosphere
    /// model's top, the ceiling is unavailable rather than a search-bound number.
    pub(super) fn ceiling_solution(
        &self,
        mass_kg: f64,
        threshold_m_s: f64,
    ) -> AexResult<CeilingMetric> {
        let upper = altitude_upper_bound(&self.scenario);
        let function = |altitude| {
            self.maximum_rate_of_climb(altitude, mass_kg)
                .map(|(_, rate)| rate - threshold_m_s)
        };
        let sea_level = function(0.0)?;
        if sea_level <= 0.0 {
            return Ok(CeilingMetric::unavailable(format!(
                "climb rate is below the {threshold_m_s:.3} m/s ceiling threshold at sea level"
            )));
        }
        let top = function(upper.value)?;
        if top >= 0.0 {
            if upper.artificial {
                return Ok(CeilingMetric::unavailable(format!(
                    "climb rate still meets the {threshold_m_s:.3} m/s ceiling threshold at the {:.0} m {} search limit",
                    upper.value, upper.source
                )));
            }
            return Ok(CeilingMetric {
                value: Some(upper.value),
                validity: MetricValidity::boundary_limited(&upper.source),
                reason: None,
            });
        }
        let value = bounded_root(0.0, upper.value, 1.0, 80, function)?;
        let evaluation = self.maximum_rate_of_climb_with_diagnostics(value, mass_kg)?;
        Ok(CeilingMetric {
            value: Some(value),
            validity: evaluation.validity,
            reason: None,
        })
    }

    /// A ceiling whose top is held up by a declared power floor is not a property of
    /// the engine's lapse law; flag it rather than report it as physical.
    pub(super) fn power_floor_warning(
        &self,
        metric: &str,
        ceiling: &CeilingMetric,
    ) -> AexResult<Option<Diagnostic>> {
        let Some(altitude) = ceiling.value else {
            return Ok(None);
        };
        let density = self.atmosphere.evaluate(altitude)?.density_kg_m3;
        if !shaft_power_floored(&self.scenario, density) {
            return Ok(None);
        }
        Ok(Some(Diagnostic::warning_with_context(
            WarningCode::CeilingReliesOnPowerFloor,
            format!(
                "{metric} at {altitude:.0} m relies on the declared minimum power fraction, not the lapse law"
            ),
            format!(
                "engine.{}.power_lapse.minimum_power_fraction",
                self.scenario.engine.profile_id()
            ),
            serde_json::json!({"metric": metric, "altitude_m": altitude}),
        )))
    }
}

pub(super) fn service_ceiling_threshold(scenario: &ResolvedScenario) -> f64 {
    match scenario.engine {
        EngineProfile::Piston(_) => 100.0 * FOOT_M / 60.0,
        EngineProfile::Turbofan(_) => 500.0 * FOOT_M / 60.0,
    }
}

#[cfg(test)]
mod tests;
