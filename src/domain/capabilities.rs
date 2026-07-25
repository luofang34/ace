use serde::Serialize;

use crate::domain::quantity::Dimension;

mod mission;

pub(crate) use mission::{
    MissionSegmentCapability, SegmentFieldCapability, SegmentFieldRequirement,
    energy_schedule_fields, mission_initial_state_fields, mission_segment, mission_segments,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentKind {
    Aircraft,
    Mission,
    Requirements,
    Profile,
    Scenario,
    Study,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct DocumentTypeCapability {
    pub(crate) id: &'static str,
    #[serde(skip)]
    pub(crate) kind: DocumentKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProfileRole {
    Engine,
    Propeller,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct ProfileTypeCapability {
    pub(crate) id: &'static str,
    pub(crate) role: ProfileRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AeroConfigurationKind {
    Clean,
    Takeoff,
    Landing,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct AeroConfigurationCapability {
    pub(crate) id: &'static str,
    #[serde(skip)]
    pub(crate) kind: AeroConfigurationKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MetricSource {
    Declared,
    Achieved,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequirementValueKind {
    Mass,
    Speed,
    Length,
    Power,
    Scalar,
}

impl RequirementValueKind {
    pub(crate) const fn canonical_unit(self) -> &'static str {
        match self {
            Self::Mass => "kg",
            Self::Speed => "m/s",
            Self::Length => "m",
            Self::Power => "W",
            Self::Scalar => "1",
        }
    }

    pub(crate) const fn dimension(self) -> Option<Dimension> {
        match self {
            Self::Mass => Some(Dimension::Mass),
            Self::Speed => Some(Dimension::Speed),
            Self::Length => Some(Dimension::Length),
            Self::Power => Some(Dimension::Power),
            Self::Scalar => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequirementMetric {
    MissionPayloadMass,
    AchievedCruiseTrueAirspeed,
    MissionCompletedDistance,
    MissionLandingFuel,
    ServiceCeiling,
    StallSpeedLanding,
    AchievedCruiseMach,
    MinimumCruiseExcessPower,
    CruiseFeasible,
    TakeoffFieldLength,
    FullPayloadRange,
    ZeroPayloadFerryRange,
    DeclaredCruiseMach,
    DeclaredCruiseTrueAirspeed,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct RequirementMetricCapability {
    pub(crate) id: &'static str,
    pub(crate) source: MetricSource,
    pub(crate) bindable: bool,
    pub(crate) value_kind: RequirementValueKind,
    pub(crate) canonical_unit: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) replacement: Option<&'static str>,
    #[serde(skip)]
    pub(crate) metric: RequirementMetric,
}

const DOCUMENT_TYPES: [DocumentTypeCapability; 6] = [
    document("aircraft", DocumentKind::Aircraft),
    document("mission", DocumentKind::Mission),
    document("requirements", DocumentKind::Requirements),
    document("profile", DocumentKind::Profile),
    document("scenario", DocumentKind::Scenario),
    document("study", DocumentKind::Study),
];

const PROFILE_TYPES: [ProfileTypeCapability; 5] = [
    profile("piston_engine", ProfileRole::Engine),
    profile("turbofan_engine", ProfileRole::Engine),
    profile("turbojet_engine", ProfileRole::Engine),
    profile("rocket_engine", ProfileRole::Engine),
    profile("propeller", ProfileRole::Propeller),
];

const AERO_CONFIGURATIONS: [AeroConfigurationCapability; 3] = [
    configuration("clean", AeroConfigurationKind::Clean),
    configuration("takeoff", AeroConfigurationKind::Takeoff),
    configuration("landing", AeroConfigurationKind::Landing),
];

const REQUIREMENT_METRICS: [RequirementMetricCapability; 14] = [
    metric(
        "mission.payload_mass",
        RequirementMetric::MissionPayloadMass,
        MetricSource::Declared,
        true,
        RequirementValueKind::Mass,
        None,
    ),
    metric(
        "performance.achieved_cruise_true_airspeed",
        RequirementMetric::AchievedCruiseTrueAirspeed,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Speed,
        None,
    ),
    metric(
        "mission.completed_distance",
        RequirementMetric::MissionCompletedDistance,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "mission.landing_fuel",
        RequirementMetric::MissionLandingFuel,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Mass,
        None,
    ),
    metric(
        "performance.service_ceiling",
        RequirementMetric::ServiceCeiling,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "performance.stall_speed_landing",
        RequirementMetric::StallSpeedLanding,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Speed,
        None,
    ),
    metric(
        "performance.achieved_cruise_mach",
        RequirementMetric::AchievedCruiseMach,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Scalar,
        None,
    ),
    metric(
        "performance.minimum_cruise_excess_power",
        RequirementMetric::MinimumCruiseExcessPower,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Power,
        None,
    ),
    metric(
        "performance.cruise_feasible",
        RequirementMetric::CruiseFeasible,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Scalar,
        None,
    ),
    metric(
        "performance.takeoff_field_length",
        RequirementMetric::TakeoffFieldLength,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "performance.full_payload_range",
        RequirementMetric::FullPayloadRange,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "performance.zero_payload_ferry_range",
        RequirementMetric::ZeroPayloadFerryRange,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "performance.cruise_mach",
        RequirementMetric::DeclaredCruiseMach,
        MetricSource::Declared,
        false,
        RequirementValueKind::Scalar,
        Some("performance.achieved_cruise_mach"),
    ),
    metric(
        "performance.cruise_true_airspeed",
        RequirementMetric::DeclaredCruiseTrueAirspeed,
        MetricSource::Declared,
        false,
        RequirementValueKind::Speed,
        Some("performance.achieved_cruise_true_airspeed"),
    ),
];

pub(crate) const REQUIREMENT_OPERATORS: [&str; 3] = ["ge", "le", "eq"];
pub(crate) const REQUIREMENT_SEVERITIES: [&str; 3] = ["hard", "soft", "report_only"];

pub(crate) const fn document_types() -> &'static [DocumentTypeCapability] {
    &DOCUMENT_TYPES
}

pub(crate) fn document_type(id: &str) -> Option<DocumentTypeCapability> {
    DOCUMENT_TYPES.iter().copied().find(|item| item.id == id)
}

pub(crate) const fn profile_types() -> &'static [ProfileTypeCapability] {
    &PROFILE_TYPES
}

pub(crate) fn profile_type(id: &str) -> Option<ProfileTypeCapability> {
    PROFILE_TYPES.iter().copied().find(|item| item.id == id)
}

pub(crate) const fn aero_configurations() -> &'static [AeroConfigurationCapability] {
    &AERO_CONFIGURATIONS
}

pub(crate) fn aero_configuration(id: &str) -> Option<AeroConfigurationCapability> {
    AERO_CONFIGURATIONS
        .iter()
        .copied()
        .find(|item| item.id == id)
}

pub(crate) const fn requirement_metrics() -> &'static [RequirementMetricCapability] {
    &REQUIREMENT_METRICS
}

pub(crate) fn requirement_metric(id: &str) -> Option<RequirementMetricCapability> {
    REQUIREMENT_METRICS
        .iter()
        .copied()
        .find(|item| item.id == id)
}

const fn document(id: &'static str, kind: DocumentKind) -> DocumentTypeCapability {
    DocumentTypeCapability { id, kind }
}

const fn profile(id: &'static str, role: ProfileRole) -> ProfileTypeCapability {
    ProfileTypeCapability { id, role }
}

const fn configuration(
    id: &'static str,
    kind: AeroConfigurationKind,
) -> AeroConfigurationCapability {
    AeroConfigurationCapability { id, kind }
}

const fn metric(
    id: &'static str,
    metric: RequirementMetric,
    source: MetricSource,
    bindable: bool,
    value_kind: RequirementValueKind,
    replacement: Option<&'static str>,
) -> RequirementMetricCapability {
    RequirementMetricCapability {
        id,
        source,
        bindable,
        value_kind,
        canonical_unit: value_kind.canonical_unit(),
        replacement,
        metric,
    }
}
