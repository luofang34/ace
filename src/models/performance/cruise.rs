use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::result::{CruiseConditionPerformance, MissionResult};
use crate::domain::schema::{MissionSegment, SegmentKind};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::domain::warning::WarningCode;
use crate::models::mission::representative_speed;

use super::validity::SearchBoundary;
use super::{PointAnalyzer, speed_upper_bound, validity_from_warnings};

pub(super) struct CruisePerformance {
    pub(super) declared_mach: Option<f64>,
    pub(super) declared_true_airspeed_m_s: Option<f64>,
    pub(super) achieved_mach: Option<f64>,
    pub(super) achieved_true_airspeed_m_s: Option<f64>,
    pub(super) minimum_excess_power_w: Option<f64>,
    pub(super) feasible: Option<bool>,
    pub(super) conditions: Vec<CruiseConditionPerformance>,
    pub(super) validity: MetricValidity,
    pub(super) warnings: Vec<Diagnostic>,
}

struct AchievedSpeedRequest {
    altitude_m: f64,
    mass_kg: f64,
    declared_speed_m_s: f64,
    minimum_level_speed_m_s: f64,
    upper: SearchBoundary,
    declared_achievable: bool,
    point_validity: MetricValidity,
}

impl PointAnalyzer {
    pub(super) fn cruise_performance(
        &self,
        mission: Option<&MissionResult>,
    ) -> AexResult<CruisePerformance> {
        let segments = self
            .scenario
            .mission
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::Cruise)
            .collect::<Vec<_>>();
        let mut conditions = Vec::with_capacity(segments.len());
        let mut warnings = Vec::new();
        for segment in &segments {
            match self.cruise_condition(segment, mission) {
                Ok((condition, condition_warnings)) => {
                    conditions.push(condition);
                    warnings.extend(condition_warnings);
                }
                Err(error) => warnings.push(Diagnostic::warning(
                    WarningCode::CruiseConditionUnsupported,
                    error.to_string(),
                    format!("mission.segments.{}", segment.id),
                )),
            }
        }
        let weakest = conditions
            .iter()
            .min_by(|left, right| left.excess_power_w.total_cmp(&right.excess_power_w));
        let all_conditions_evaluated = conditions.len() == segments.len();
        let all_conditions_achievable = all_conditions_evaluated
            && conditions
                .iter()
                .all(|condition| condition.achieved_mach.is_some());
        let achieved_mach = if all_conditions_achievable {
            weakest.and_then(|condition| condition.achieved_mach)
        } else {
            None
        };
        let achieved_true_airspeed_m_s = if all_conditions_achievable {
            weakest.and_then(|condition| condition.achieved_true_airspeed_m_s)
        } else {
            None
        };
        let minimum_excess_power_w = if all_conditions_evaluated {
            weakest.map(|condition| condition.excess_power_w)
        } else {
            None
        };
        let validity = aggregate_validity(conditions.iter().map(|condition| &condition.validity));
        let declared_mach = segments.first().and_then(|segment| {
            segment
                .mach
                .or_else(|| conditions.first().map(|condition| condition.declared_mach))
        });
        let declared_true_airspeed_m_s = segments.first().and_then(|segment| {
            segment.true_airspeed_m_s.or_else(|| {
                conditions
                    .first()
                    .map(|condition| condition.declared_true_airspeed_m_s)
            })
        });
        Ok(CruisePerformance {
            declared_mach,
            declared_true_airspeed_m_s,
            achieved_mach,
            achieved_true_airspeed_m_s,
            minimum_excess_power_w,
            feasible: (!segments.is_empty()).then(|| {
                all_conditions_evaluated && conditions.iter().all(|condition| condition.feasible)
            }),
            conditions,
            validity,
            warnings,
        })
    }

    fn cruise_condition(
        &self,
        segment: &MissionSegment,
        mission: Option<&MissionResult>,
    ) -> AexResult<(CruiseConditionPerformance, Vec<Diagnostic>)> {
        let result = mission
            .into_iter()
            .flat_map(|mission| &mission.segments)
            .find(|result| result.segment_id == segment.id);
        let altitude_m = segment
            .altitude_m
            .or_else(|| result.map(|result| result.end_altitude_m))
            .unwrap_or(0.0);
        let mass_kg = result.map_or(
            self.scenario.aircraft.mass.maximum_takeoff_mass_kg,
            |result| result.start_mass_kg,
        );
        let declared_true_airspeed_m_s = representative_speed(segment, &self.scenario, altitude_m)?;
        let atmosphere = self.atmosphere.evaluate(altitude_m)?;
        let declared_mach = declared_true_airspeed_m_s / atmosphere.speed_of_sound_m_s;
        let evaluation = self.point(altitude_m, declared_true_airspeed_m_s, mass_kg, "clean")?;
        let upper = speed_upper_bound(&self.scenario, &atmosphere);
        let altitude_supported = self
            .scenario
            .aircraft
            .limits
            .maximum_operating_altitude_m
            .is_none_or(|limit| altitude_m <= limit);
        let declared_speed_achievable =
            evaluation.excess_power_w >= -1.0e-9 && declared_true_airspeed_m_s <= upper.value;
        let feasible = declared_speed_achievable && altitude_supported;
        let point_validity = validity_from_warnings(&evaluation.warnings);
        let mut condition_warnings = evaluation.warnings;
        let (achieved_true_airspeed_m_s, validity, capability_warning) =
            self.achieved_speed(AchievedSpeedRequest {
                altitude_m,
                mass_kg,
                declared_speed_m_s: declared_true_airspeed_m_s,
                minimum_level_speed_m_s: evaluation.stall_speed_m_s * 1.05,
                upper,
                declared_achievable: declared_speed_achievable,
                point_validity,
            })?;
        condition_warnings.extend(capability_warning);
        if !altitude_supported {
            condition_warnings.push(Diagnostic::warning(
                WarningCode::CruiseAltitudeLimitExceeded,
                format!(
                    "Cruise altitude {altitude_m} m exceeds the declared maximum operating altitude."
                ),
                "aircraft.limits.maximum_operating_altitude",
            ));
        }
        let warnings = condition_warnings
            .into_iter()
            .map(|warning| scope_warning(warning, &segment.id))
            .collect();
        Ok((
            CruiseConditionPerformance {
                segment_id: segment.id.clone(),
                altitude_m,
                mass_kg,
                declared_true_airspeed_m_s,
                declared_mach,
                achieved_true_airspeed_m_s,
                achieved_mach: achieved_true_airspeed_m_s
                    .map(|speed| speed / atmosphere.speed_of_sound_m_s),
                excess_power_w: evaluation.excess_power_w,
                feasible,
                validity,
            },
            warnings,
        ))
    }

    fn achieved_speed(
        &self,
        request: AchievedSpeedRequest,
    ) -> AexResult<(Option<f64>, MetricValidity, Option<Diagnostic>)> {
        if request.declared_achievable {
            return Ok((
                Some(request.declared_speed_m_s),
                request.point_validity,
                None,
            ));
        }
        if request.upper.value < request.minimum_level_speed_m_s {
            let message = format!(
                "{} permits at most {:.3} m/s, below the minimum level-flight speed {:.3} m/s",
                request.upper.source, request.upper.value, request.minimum_level_speed_m_s
            );
            return Ok(unavailable_capability(message, request.point_validity));
        }
        match self.maximum_level_speed_solution(request.altitude_m, request.mass_kg) {
            Ok(solution) => Ok((
                Some(solution.value),
                combine_validity(&request.point_validity, &solution.validity),
                None,
            )),
            Err(crate::domain::diagnostic::AexError::Analysis {
                code: "NO_MAXIMUM_SPEED_INTERSECTION",
                message,
            }) => Ok(unavailable_capability(message, request.point_validity)),
            Err(error) => Err(error),
        }
    }
}

fn unavailable_capability(
    message: String,
    validity: MetricValidity,
) -> (Option<f64>, MetricValidity, Option<Diagnostic>) {
    (
        None,
        validity,
        Some(Diagnostic::warning(
            WarningCode::CruiseCapabilityUnavailable,
            message,
            "performance.achieved_cruise_true_airspeed",
        )),
    )
}

fn aggregate_validity<'a>(validities: impl Iterator<Item = &'a MetricValidity>) -> MetricValidity {
    validities
        .cloned()
        .reduce(|left, right| combine_validity(&left, &right))
        .unwrap_or_default()
}

fn combine_validity(left: &MetricValidity, right: &MetricValidity) -> MetricValidity {
    match (left.status, right.status) {
        (ValidityStatus::BoundaryLimited, _) => left.clone(),
        (_, ValidityStatus::BoundaryLimited) => right.clone(),
        (ValidityStatus::Extrapolated, _) | (_, ValidityStatus::Extrapolated) => {
            MetricValidity::extrapolated()
        }
        (ValidityStatus::Valid, ValidityStatus::Valid) => MetricValidity::default(),
    }
}

fn scope_warning(mut warning: Diagnostic, segment_id: &str) -> Diagnostic {
    let base = format!("mission.segments.{segment_id}");
    warning.path = Some(match warning.path {
        Some(path) => format!("{base}.{path}"),
        None => base,
    });
    warning
}
