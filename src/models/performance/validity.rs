use crate::domain::diagnostic::Diagnostic;
use crate::domain::result::AtmosphereState;
use crate::domain::schema::ResolvedScenario;
use crate::domain::validity::{MetricValidity, ValidityStatus};

#[derive(Debug, Clone, Copy)]
pub(super) struct SearchBoundary {
    pub(super) value: f64,
    pub(super) source: &'static str,
    artificial: bool,
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
        .any(|diagnostic| diagnostic.code == "MODEL_EXTRAPOLATION")
    {
        MetricValidity::extrapolated()
    } else {
        MetricValidity::default()
    }
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
