use serde::Serialize;

use crate::domain::quantity::Dimension;

pub(crate) const REQUIREMENT_OPERATORS: [&str; 3] = ["ge", "le", "eq"];
pub(crate) const REQUIREMENT_SEVERITIES: [&str; 3] = ["hard", "soft", "report_only"];

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
    Time,
    Power,
    Scalar,
}

impl RequirementValueKind {
    pub(crate) const fn canonical_unit(self) -> &'static str {
        match self {
            Self::Mass => "kg",
            Self::Speed => "m/s",
            Self::Length => "m",
            Self::Time => "s",
            Self::Power => "W",
            Self::Scalar => "1",
        }
    }

    pub(crate) const fn dimension(self) -> Option<Dimension> {
        match self {
            Self::Mass => Some(Dimension::Mass),
            Self::Speed => Some(Dimension::Speed),
            Self::Length => Some(Dimension::Length),
            Self::Time => Some(Dimension::Time),
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
    MissionReserveDuration,
    ServiceCeiling,
    StallSpeedLanding,
    AchievedCruiseMach,
    MinimumCruiseExcessPower,
    CruiseFeasible,
    TakeoffFieldLength,
    LandingFieldLength,
    AllEngineClimbGradient,
    OeiSecondSegmentClimbGradient,
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

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub(crate) enum RequirementTemplateValue {
    Quantity(&'static str),
    Scalar(f64),
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct EngineCountValueCapability {
    pub(crate) engine_count: u32,
    pub(crate) value: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct RequirementProvenanceCapability {
    pub(crate) kind: &'static str,
    pub(crate) source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) citation: Option<&'static str>,
    pub(crate) non_regulatory: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct RequirementTemplateItemCapability {
    pub(crate) id: &'static str,
    pub(crate) metric: &'static str,
    pub(crate) operator: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<RequirementTemplateValue>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub(crate) values_by_engine_count: &'static [EngineCountValueCapability],
    pub(crate) severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) weight: Option<f64>,
    pub(crate) provenance: RequirementProvenanceCapability,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct TemplateParameterCapability {
    pub(crate) name: &'static str,
    pub(crate) required: bool,
    pub(crate) allowed_values: &'static [u32],
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct RequirementTemplateCapability {
    pub(crate) id: &'static str,
    pub(crate) version: u32,
    pub(crate) display_name: &'static str,
    pub(crate) category: &'static str,
    pub(crate) certification_use: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parameter: Option<TemplateParameterCapability>,
    pub(crate) items: &'static [RequirementTemplateItemCapability],
}

const DESIGNER_DEFAULT: RequirementProvenanceCapability = RequirementProvenanceCapability {
    kind: "designer_default",
    source: "ACE conceptual-design default",
    citation: None,
    non_regulatory: true,
};

const TRANSPORT_OEI_SOURCE: RequirementProvenanceCapability = RequirementProvenanceCapability {
    kind: "regulatory_derived",
    source: "14 CFR 25.121(b), second-segment one-engine-inoperative climb",
    citation: Some("https://www.ecfr.gov/current/title-14/section-25.121"),
    non_regulatory: false,
};

const OEI_GRADIENTS: [EngineCountValueCapability; 3] = [
    EngineCountValueCapability {
        engine_count: 2,
        value: 0.024,
    },
    EngineCountValueCapability {
        engine_count: 3,
        value: 0.027,
    },
    EngineCountValueCapability {
        engine_count: 4,
        value: 0.030,
    },
];

const LIGHT_ITEMS: [RequirementTemplateItemCapability; 5] = [
    item(
        "stall_speed_landing",
        "performance.stall_speed_landing",
        "le",
        RequirementTemplateValue::Quantity("61 kt"),
    ),
    item(
        "takeoff_field_length",
        "performance.takeoff_field_length",
        "le",
        RequirementTemplateValue::Quantity("2500 ft"),
    ),
    item(
        "landing_field_length",
        "performance.landing_field_length",
        "le",
        RequirementTemplateValue::Quantity("2500 ft"),
    ),
    item(
        "all_engine_climb_gradient",
        "performance.all_engine_climb_gradient",
        "ge",
        RequirementTemplateValue::Scalar(0.05),
    ),
    item(
        "reserve_duration",
        "mission.reserve_duration",
        "ge",
        RequirementTemplateValue::Quantity("45 min"),
    ),
];

const TRANSPORT_ITEMS: [RequirementTemplateItemCapability; 5] = [
    item(
        "stall_speed_landing",
        "performance.stall_speed_landing",
        "le",
        RequirementTemplateValue::Quantity("150 kt"),
    ),
    item(
        "takeoff_field_length",
        "performance.takeoff_field_length",
        "le",
        RequirementTemplateValue::Quantity("11000 ft"),
    ),
    item(
        "landing_field_length",
        "performance.landing_field_length",
        "le",
        RequirementTemplateValue::Quantity("8000 ft"),
    ),
    item(
        "reserve_duration",
        "mission.reserve_duration",
        "ge",
        RequirementTemplateValue::Quantity("30 min"),
    ),
    RequirementTemplateItemCapability {
        id: "oei_second_segment_climb_gradient",
        metric: "performance.oei_second_segment_climb_gradient",
        operator: "ge",
        value: None,
        values_by_engine_count: &OEI_GRADIENTS,
        severity: "hard",
        weight: None,
        provenance: TRANSPORT_OEI_SOURCE,
    },
];

const TEMPLATES: [RequirementTemplateCapability; 2] = [
    RequirementTemplateCapability {
        id: "light_aircraft_conceptual",
        version: 1,
        display_name: "Light-aircraft conceptual defaults",
        category: "normal-category light aircraft",
        certification_use: "conceptual_screen_only",
        parameter: None,
        items: &LIGHT_ITEMS,
    },
    RequirementTemplateCapability {
        id: "transport_conceptual",
        version: 1,
        display_name: "Transport-aircraft conceptual defaults",
        category: "transport",
        certification_use: "conceptual_screen_only",
        parameter: Some(TemplateParameterCapability {
            name: "engine_count",
            required: true,
            allowed_values: &[2, 3, 4],
        }),
        items: &TRANSPORT_ITEMS,
    },
];

const REQUIREMENT_METRICS: [RequirementMetricCapability; 18] = [
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
        "mission.reserve_duration",
        RequirementMetric::MissionReserveDuration,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Time,
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
        "performance.landing_field_length",
        RequirementMetric::LandingFieldLength,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Length,
        None,
    ),
    metric(
        "performance.all_engine_climb_gradient",
        RequirementMetric::AllEngineClimbGradient,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Scalar,
        None,
    ),
    metric(
        "performance.oei_second_segment_climb_gradient",
        RequirementMetric::OeiSecondSegmentClimbGradient,
        MetricSource::Achieved,
        true,
        RequirementValueKind::Scalar,
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

pub(crate) const fn requirement_metrics() -> &'static [RequirementMetricCapability] {
    &REQUIREMENT_METRICS
}

pub(crate) fn requirement_metric(id: &str) -> Option<RequirementMetricCapability> {
    REQUIREMENT_METRICS
        .iter()
        .copied()
        .find(|item| item.id == id)
}

pub(crate) const fn requirement_templates() -> &'static [RequirementTemplateCapability] {
    &TEMPLATES
}

pub(crate) fn requirement_template(
    id: &str,
    version: u32,
) -> Option<RequirementTemplateCapability> {
    TEMPLATES
        .iter()
        .copied()
        .find(|template| template.id == id && template.version == version)
}

const fn item(
    id: &'static str,
    metric_id: &'static str,
    operator: &'static str,
    value: RequirementTemplateValue,
) -> RequirementTemplateItemCapability {
    RequirementTemplateItemCapability {
        id,
        metric: metric_id,
        operator,
        value: Some(value),
        values_by_engine_count: &[],
        severity: "soft",
        weight: None,
        provenance: DESIGNER_DEFAULT,
    }
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
