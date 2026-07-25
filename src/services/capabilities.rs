use serde::Serialize;

use crate::backends::contracts::BackendDescriptor;
use crate::domain::capabilities::{
    AeroConfigurationCapability, DocumentTypeCapability, MissionSegmentCapability,
    ProfileTypeCapability, REQUIREMENT_OPERATORS, REQUIREMENT_SEVERITIES,
    RequirementMetricCapability, aero_configurations, document_types, mission_segments,
    profile_types, requirement_metrics,
};
#[cfg(test)]
use crate::domain::capabilities::{MetricSource, ProfileRole, SegmentFieldRequirement};
use crate::domain::diagnostic::AexResult;
use crate::domain::validity::ModelValidityDomain;
use crate::domain::warning::warning_policy;
use crate::models::validity::registered_model_domains;
use crate::services::analysis::ApplicationService;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WarningCapability {
    pub(crate) code: &'static str,
    pub(crate) promotes_in_strict: bool,
    pub(crate) description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilitiesManifest {
    pub(crate) schema_version: u32,
    pub(crate) document_types: &'static [DocumentTypeCapability],
    pub(crate) profile_types: &'static [ProfileTypeCapability],
    pub(crate) configurations: &'static [AeroConfigurationCapability],
    pub(crate) mission_segments: &'static [MissionSegmentCapability],
    pub(crate) requirement_metrics: &'static [RequirementMetricCapability],
    pub(crate) requirement_operators: &'static [&'static str],
    pub(crate) requirement_severities: &'static [&'static str],
    pub(crate) backends: Vec<BackendDescriptor>,
    pub(crate) model_domains: Vec<ModelValidityDomain>,
    pub(crate) strict_warning_policy: Vec<WarningCapability>,
}

impl ApplicationService {
    pub(crate) fn capabilities(&self) -> AexResult<CapabilitiesManifest> {
        manifest(self.list_analysis_backends())
    }
}

fn manifest(backends: Vec<BackendDescriptor>) -> AexResult<CapabilitiesManifest> {
    let strict_warning_policy = warning_policy()
        .iter()
        .map(|entry| WarningCapability {
            code: entry.code.as_str(),
            promotes_in_strict: entry.promotes_in_strict,
            description: entry.description,
        })
        .collect();
    Ok(CapabilitiesManifest {
        schema_version: 1,
        document_types: document_types(),
        profile_types: profile_types(),
        configurations: aero_configurations(),
        mission_segments: mission_segments(),
        requirement_metrics: requirement_metrics(),
        requirement_operators: &REQUIREMENT_OPERATORS,
        requirement_severities: &REQUIREMENT_SEVERITIES,
        backends,
        model_domains: registered_model_domains()?,
        strict_warning_policy,
    })
}

#[cfg(test)]
pub(super) fn reference_markdown(manifest: &CapabilitiesManifest) -> String {
    let documents = manifest
        .document_types
        .iter()
        .map(|item| format!("`{}`", item.id))
        .collect::<Vec<_>>()
        .join(", ");
    let profiles = manifest
        .profile_types
        .iter()
        .map(|item| format!("| `{}` | `{}` |\n", item.id, profile_role(item.role)))
        .collect::<String>();
    let configurations = manifest
        .configurations
        .iter()
        .map(|item| format!("`{}`", item.id))
        .collect::<Vec<_>>()
        .join(", ");
    let segments = manifest
        .mission_segments
        .iter()
        .map(segment_markdown)
        .collect::<String>();
    let metrics = manifest
        .requirement_metrics
        .iter()
        .map(|item| {
            let replacement = item
                .replacement
                .map_or_else(|| "—".to_owned(), |value| format!("`{value}`"));
            format!(
                "| `{}` | `{}` | {} | `{}` | {} |\n",
                item.id,
                metric_source(item.source),
                if item.bindable { "yes" } else { "no" },
                item.canonical_unit,
                replacement
            )
        })
        .collect::<String>();
    let domains = manifest
        .model_domains
        .iter()
        .map(|domain| format!("- `{}`\n", domain.model_id))
        .collect::<String>();
    let backends = manifest
        .backends
        .iter()
        .map(|backend| format!("- `{}`\n", backend.id))
        .collect::<String>();
    let operators = code_list(manifest.requirement_operators);
    let severities = code_list(manifest.requirement_severities);
    format!(
        "# Capability manifest\n\n\
         The CLI command `aex capabilities --format json` and MCP tool \
         `get_capabilities` return the same versioned manifest.\n\n\
         ## Documents\n\n{documents}\n\n\
         ## Profile types\n\n\
         | Type | Role |\n| --- | --- |\n{profiles}\n\
         ## Aerodynamic configurations\n\n{configurations}\n\n\
         ## Mission segments\n\n\
         | Type | Legal fields |\n| --- | --- |\n{segments}\n\
         Power/thrust fractions on climb, cruise, loiter, and reserve constrain the \
         mission-power feasibility screen. Quasi-steady cruise/loiter fuel burn follows \
         the aerodynamic power required and is not scaled directly by throttle.\n\n\
         ## Requirement metrics\n\n\
         | Metric | Source | Bindable | Unit | Replacement |\n\
         | --- | --- | --- | --- | --- |\n{metrics}\n\
         Operators: {operators}. Severities: {severities}.\n\n\
         ## Backends\n\n{backends}\n\
         ## Registered model domains\n\n{domains}\n\
         Strict-warning decisions are listed in [the generated warning policy](strict-warning-policy.md).\n"
    )
}

#[cfg(test)]
fn code_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
fn segment_markdown(segment: &MissionSegmentCapability) -> String {
    let fields = segment
        .fields
        .iter()
        .map(|field| {
            format!(
                "`{}` ({})",
                field.name,
                field_requirement(field.requirement)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("| `{}` | {} |\n", segment.segment_type, fields)
}

#[cfg(test)]
const fn profile_role(role: ProfileRole) -> &'static str {
    match role {
        ProfileRole::Engine => "engine",
        ProfileRole::Propeller => "propeller",
    }
}

#[cfg(test)]
const fn metric_source(source: MetricSource) -> &'static str {
    match source {
        MetricSource::Declared => "declared",
        MetricSource::Achieved => "achieved",
    }
}

#[cfg(test)]
const fn field_requirement(requirement: SegmentFieldRequirement) -> &'static str {
    match requirement {
        SegmentFieldRequirement::Required => "required",
        SegmentFieldRequirement::Optional => "optional",
        SegmentFieldRequirement::ExactlyOne => "exactly_one",
    }
}

#[cfg(test)]
mod tests;
