use super::solver::bounded_root;
use super::validity::{
    declared_altitude_limits, from_warnings as validity_from_warnings, speed_upper_bound,
};
use super::{ClimbRateEvaluation, PointAnalyzer, extend_unique_diagnostics};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::{FOOT_M, GRAVITY_M_S2};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::stall_speed_m_s;
use crate::models::atmosphere::MAXIMUM_ALTITUDE_M;
use crate::models::propulsion::{OperatingMode, shaft_power_floored};

/// Altitude step for bracketing the first ceiling root.
const CEILING_SEARCH_STEP_M: f64 = 500.0;

/// A solved ceiling, or the reason no ceiling exists inside the atmosphere model.
#[derive(Debug)]
pub(super) struct CeilingMetric {
    pub(super) value: Option<f64>,
    pub(super) validity: MetricValidity,
    unavailable: Option<Unavailable>,
}

#[derive(Debug, Clone, Copy)]
enum Unavailable {
    NoClimbAtSeaLevel { threshold_m_s: f64 },
    AboveAtmosphere { threshold_m_s: f64 },
}

impl Unavailable {
    fn message(self) -> String {
        match self {
            Self::NoClimbAtSeaLevel { threshold_m_s } => format!(
                "climb rate is below the {threshold_m_s:.3} m/s ceiling threshold at sea level"
            ),
            Self::AboveAtmosphere { threshold_m_s } => format!(
                "climb rate still meets the {threshold_m_s:.3} m/s ceiling threshold at {MAXIMUM_ALTITUDE_M:.0} m, the top of the standard atmosphere"
            ),
        }
    }
}

/// A ceiling as published (POH-style): the physical root cut off at the lowest
/// declared aircraft or engine altitude limit.
#[derive(Debug)]
pub(super) struct PublishedCeiling {
    pub(super) value: Option<f64>,
    pub(super) limited_by: Option<String>,
    pub(super) validity: MetricValidity,
}

impl CeilingMetric {
    fn reason_is_above_search(&self) -> bool {
        matches!(self.unavailable, Some(Unavailable::AboveAtmosphere { .. }))
    }

    fn unavailable(reason: Unavailable) -> Self {
        Self {
            value: None,
            validity: MetricValidity::unsupported(),
            unavailable: Some(reason),
        }
    }

    pub(super) fn diagnostic(&self, metric: &str) -> Option<Diagnostic> {
        self.unavailable.map(|reason| {
            Diagnostic::warning_with_context(
                WarningCode::CeilingUnavailable,
                format!("{metric} is unavailable: {}", reason.message()),
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
        // The lower search speed is the 1.05 V_stall margin, a real constraint on the
        // climb optimum rather than an artificial cap, so it keeps the point's validity.
        let validity = if best_index == 99 {
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

    /// Solves for the lowest altitude where the best climb rate falls to
    /// `threshold_m_s`, stepping up through the whole standard atmosphere and
    /// bisecting the first sign change. Declared altitude limits never clamp this
    /// physical result; [`Self::published_ceiling`] applies them separately, and a
    /// root beyond an engine's declared envelope carries extrapolated validity.
    pub(super) fn ceiling_solution(
        &self,
        mass_kg: f64,
        threshold_m_s: f64,
    ) -> AexResult<CeilingMetric> {
        let function = |altitude| {
            self.maximum_rate_of_climb(altitude, mass_kg)
                .map(|(_, rate)| rate - threshold_m_s)
        };
        if function(0.0)? <= 0.0 {
            return Ok(CeilingMetric::unavailable(Unavailable::NoClimbAtSeaLevel {
                threshold_m_s,
            }));
        }
        let mut lower = 0.0;
        while lower < MAXIMUM_ALTITUDE_M {
            let upper = (lower + CEILING_SEARCH_STEP_M).min(MAXIMUM_ALTITUDE_M);
            if function(upper)? < 0.0 {
                let value = bounded_root(lower, upper, 1.0, 80, function)?;
                let evaluation = self.maximum_rate_of_climb_with_diagnostics(value, mass_kg)?;
                return Ok(CeilingMetric {
                    value: Some(value),
                    validity: evaluation.validity,
                    unavailable: None,
                });
            }
            lower = upper;
        }
        Ok(CeilingMetric::unavailable(Unavailable::AboveAtmosphere {
            threshold_m_s,
        }))
    }

    /// The published ceiling: the physical root cut off at the lowest declared
    /// aircraft or engine altitude limit, naming the limit when it binds. A binding
    /// declared limit is authoritative, so the cut value is valid even where the
    /// physical root lies beyond an engine's envelope.
    pub(super) fn published_ceiling(&self, physical: &CeilingMetric) -> PublishedCeiling {
        let limit = declared_altitude_limits(&self.scenario)
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0));
        let cut = |limit, source| PublishedCeiling {
            value: Some(limit),
            limited_by: Some(source),
            validity: MetricValidity::default(),
        };
        match (physical.value, limit) {
            (Some(root), Some((limit, source))) if limit < root => cut(limit, source),
            // Climb performance beyond the atmosphere top is still cut off by a declared
            // limit inside it; a limit above the top cannot be confirmed either way.
            (None, Some((limit, source)))
                if physical.reason_is_above_search() && limit <= MAXIMUM_ALTITUDE_M =>
            {
                cut(limit, source)
            }
            (value, _) => PublishedCeiling {
                value,
                limited_by: None,
                validity: physical.validity.clone(),
            },
        }
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
