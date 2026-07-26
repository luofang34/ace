use serde::Serialize;

mod mission;
mod requirements;

pub(crate) use mission::{
    MissionSegmentCapability, SegmentFieldCapability, SegmentFieldRequirement,
    energy_schedule_fields, mission_initial_state_fields, mission_segment, mission_segments,
};
#[cfg(test)]
pub(crate) use requirements::MetricSource;
pub(crate) use requirements::{
    REQUIREMENT_OPERATORS, REQUIREMENT_SEVERITIES, RequirementMetric, RequirementMetricCapability,
    RequirementTemplateCapability, RequirementTemplateItemCapability, RequirementTemplateValue,
    RequirementValueKind, requirement_metric, requirement_metrics, requirement_template,
    requirement_templates,
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
