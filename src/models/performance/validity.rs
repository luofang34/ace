use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::result::AtmosphereState;
use crate::domain::schema::{Aircraft, ResolvedScenario};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::coefficient_evaluation_at_mach;

use super::cruise::CruisePerformance;
use super::{
    ABSOLUTE_CEILING_METRIC, ACHIEVED_CRUISE_MACH_METRIC, ACHIEVED_CRUISE_TAS_METRIC,
    CRUISE_FEASIBLE_METRIC, MAXIMUM_SPEED_METRIC, MINIMUM_CRUISE_EXCESS_POWER_METRIC,
    SERVICE_CEILING_METRIC, SolvedMetric,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct SearchBoundary {
    pub(super) value: f64,
    pub(super) source: &'static str,
    artificial: bool,
}

pub(super) struct ReferenceMetricValidity {
    pub(super) clean: MetricValidity,
    pub(super) landing: MetricValidity,
    pub(super) warnings: Vec<Diagnostic>,
}

pub(super) struct PointReferenceValidity {
    pub(super) metric_validity: BTreeMap<String, MetricValidity>,
    pub(super) warnings: Vec<Diagnostic>,
}

impl SearchBoundary {
    pub(super) fn validity(self) -> MetricValidity {
        if self.artificial {
            MetricValidity::boundary_limited(self.source)
        } else {
            MetricValidity::default()
        }
    }
}

pub(super) fn speed_upper_bound(
    scenario: &ResolvedScenario,
    atmosphere: &AtmosphereState,
) -> SearchBoundary {
    let declared_mach = scenario.aircraft.limits.maximum_operating_mach;
    let mach_limit = declared_mach.unwrap_or(0.95) * atmosphere.speed_of_sound_m_s;
    match scenario.aircraft.limits.maximum_operating_speed_m_s {
        Some(speed_limit) if speed_limit <= mach_limit => SearchBoundary {
            value: speed_limit,
            source: "aircraft.limits.maximum_operating_speed",
            artificial: false,
        },
        _ => SearchBoundary {
            value: mach_limit,
            source: if declared_mach.is_some() {
                "aircraft.limits.maximum_operating_mach"
            } else {
                "performance.maximum_speed_search.default_mach"
            },
            artificial: declared_mach.is_none(),
        },
    }
}

pub(super) fn altitude_upper_bound(scenario: &ResolvedScenario) -> SearchBoundary {
    match scenario.aircraft.limits.maximum_operating_altitude_m {
        Some(limit) if limit < 19_900.0 => SearchBoundary {
            value: limit,
            source: "aircraft.limits.maximum_operating_altitude",
            artificial: false,
        },
        _ => SearchBoundary {
            value: 19_900.0,
            source: "atmosphere.isa1976.maximum_altitude",
            artificial: true,
        },
    }
}

pub(super) fn from_warnings(warnings: &[Diagnostic]) -> MetricValidity {
    if warnings
        .iter()
        .any(|diagnostic| diagnostic.code == WarningCode::ModelExtrapolation.as_str())
    {
        MetricValidity::extrapolated()
    } else {
        MetricValidity::default()
    }
}

pub(super) fn reference_metric_validity(aircraft: &Aircraft) -> AexResult<ReferenceMetricValidity> {
    let clean = coefficient_evaluation_at_mach(aircraft, "clean", 0.0)?;
    let landing = coefficient_evaluation_at_mach(aircraft, "landing", 0.0)?;
    let clean_validity = from_warnings(&clean.warnings);
    let landing_validity = from_warnings(&landing.warnings);
    let mut warnings = clean.warnings;
    warnings.extend(landing.warnings);
    Ok(ReferenceMetricValidity {
        clean: clean_validity,
        landing: landing_validity,
        warnings,
    })
}

pub(super) fn point_reference_validity(
    aircraft: &Aircraft,
    configuration: &str,
) -> AexResult<PointReferenceValidity> {
    let selected = coefficient_evaluation_at_mach(aircraft, configuration, 0.0)?;
    let selected_validity = from_warnings(&selected.warnings);
    let mut warnings = selected.warnings;
    let clean_validity = if configuration == "clean" {
        selected_validity.clone()
    } else {
        let clean = coefficient_evaluation_at_mach(aircraft, "clean", 0.0)?;
        let validity = from_warnings(&clean.warnings);
        warnings.extend(clean.warnings);
        validity
    };
    Ok(PointReferenceValidity {
        metric_validity: BTreeMap::from([
            ("performance.stall_speed".to_owned(), selected_validity),
            (
                "performance.best_glide_speed".to_owned(),
                clean_validity.clone(),
            ),
            (
                "aerodynamics.maximum_lift_to_drag_ratio".to_owned(),
                clean_validity,
            ),
        ]),
        warnings,
    })
}

pub(super) fn aggregate<'a>(
    validities: impl IntoIterator<Item = &'a MetricValidity>,
) -> ValidityStatus {
    let mut aggregate = ValidityStatus::Valid;
    for validity in validities {
        match validity.status {
            ValidityStatus::BoundaryLimited => return ValidityStatus::BoundaryLimited,
            ValidityStatus::Extrapolated => aggregate = ValidityStatus::Extrapolated,
            ValidityStatus::Valid => {}
        }
    }
    aggregate
}

pub(super) fn summary_metric_validity(
    maximum_speed: &SolvedMetric,
    service_ceiling: &SolvedMetric,
    absolute_ceiling: &SolvedMetric,
    cruise: &CruisePerformance,
    reference: &ReferenceMetricValidity,
    takeoff_field: MetricValidity,
    landing_field: MetricValidity,
) -> BTreeMap<String, MetricValidity> {
    let mut validity = BTreeMap::from([
        (
            MAXIMUM_SPEED_METRIC.to_owned(),
            maximum_speed.validity.clone(),
        ),
        (
            SERVICE_CEILING_METRIC.to_owned(),
            service_ceiling.validity.clone(),
        ),
        (
            ABSOLUTE_CEILING_METRIC.to_owned(),
            absolute_ceiling.validity.clone(),
        ),
    ]);
    for (present, metric) in [
        (cruise.achieved_mach.is_some(), ACHIEVED_CRUISE_MACH_METRIC),
        (
            cruise.achieved_true_airspeed_m_s.is_some(),
            ACHIEVED_CRUISE_TAS_METRIC,
        ),
        (
            cruise.minimum_excess_power_w.is_some(),
            MINIMUM_CRUISE_EXCESS_POWER_METRIC,
        ),
        (cruise.feasible.is_some(), CRUISE_FEASIBLE_METRIC),
    ] {
        if present {
            validity.insert(metric.to_owned(), cruise.validity.clone());
        }
    }
    for metric in [
        "performance.stall_speed",
        "performance.stall_speed_clean",
        "performance.best_glide_speed",
        "performance.minimum_power_speed",
        "aerodynamics.maximum_lift_to_drag_ratio",
    ] {
        validity.insert(metric.to_owned(), reference.clean.clone());
    }
    validity.insert(
        "performance.stall_speed_landing".to_owned(),
        reference.landing.clone(),
    );
    validity.insert("performance.takeoff_field_length".to_owned(), takeoff_field);
    validity.insert("performance.landing_field_length".to_owned(), landing_field);
    validity
}
