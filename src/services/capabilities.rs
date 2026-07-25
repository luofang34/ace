use serde::Serialize;

use crate::backends::contracts::BackendDescriptor;
use crate::domain::capabilities::{
    AeroConfigurationCapability, DocumentTypeCapability, MissionSegmentCapability,
    ProfileTypeCapability, REQUIREMENT_OPERATORS, REQUIREMENT_SEVERITIES,
    RequirementMetricCapability, RequirementTemplateCapability, SegmentFieldCapability,
    aero_configurations, document_types, energy_schedule_fields, mission_initial_state_fields,
    mission_segments, profile_types, requirement_metrics, requirement_templates,
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
    pub(crate) mission_initial_state_fields: &'static [SegmentFieldCapability],
    pub(crate) energy_schedule_fields: &'static [SegmentFieldCapability],
    pub(crate) mission_segments: &'static [MissionSegmentCapability],
    pub(crate) requirement_templates: &'static [RequirementTemplateCapability],
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
        mission_initial_state_fields: mission_initial_state_fields(),
        energy_schedule_fields: energy_schedule_fields(),
        mission_segments: mission_segments(),
        requirement_templates: requirement_templates(),
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
    let initial_state_fields = initial_state_markdown(manifest.mission_initial_state_fields);
    let energy_schedule_fields = initial_state_markdown(manifest.energy_schedule_fields);
    let metrics = requirement_metrics_markdown(manifest);
    let templates = manifest
        .requirement_templates
        .iter()
        .map(template_markdown)
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
    let (operators, severities) = requirement_lists(manifest);
    let mission_notes = mission_notes();
    format!(
        "# Capability manifest\n\n\
         The CLI command `aex capabilities --format json` and MCP tool \
         `get_capabilities` return the same versioned manifest.\n\n\
         ## Documents\n\n{documents}\n\n\
         ## Profile types\n\n\
         | Type | Role |\n| --- | --- |\n{profiles}\n\
         ## Aerodynamic configurations\n\n{configurations}\n\n\
         ## Mission segments\n\n\
         Initial-state fields: {initial_state_fields}.\n\n\
         Energy-schedule point fields: {energy_schedule_fields}.\n\n\
         | Type | Legal fields |\n| --- | --- |\n{segments}\n\
         Fields in the same `at_most_one` group are mutually exclusive. Fields in an \
         `exactly_one` group require one and only one representation. Unlisted fields are rejected.\n\n\
         A declared power or thrust fraction of zero means engine off and produces zero \
         modeled propulsion output and fuel flow.\n\n\
         {mission_notes}\n\n\
         ## Requirement templates\n\n\
         All shipped templates are conceptual screens, not certification findings.\n\n\
         Transport OEI second-segment gradients are regulatory-derived from \
         [14 CFR 25.121(b)](https://www.ecfr.gov/current/title-14/section-25.121); \
         the implemented calculation remains a conceptual screen.\n\n\
         | Template | Version | Category | Parameter | Items |\n\
         | --- | --- | --- | --- | --- |\n{templates}\n\
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
fn requirement_metrics_markdown(manifest: &CapabilitiesManifest) -> String {
    manifest
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
        .collect()
}

#[cfg(test)]
fn template_markdown(template: &RequirementTemplateCapability) -> String {
    let parameter = template.parameter.map_or_else(
        || "—".to_owned(),
        |parameter| {
            let allowed = parameter
                .allowed_values
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/");
            format!("`{}` ({allowed})", parameter.name)
        },
    );
    let items = template
        .items
        .iter()
        .map(|item| {
            let value = item.value.map_or_else(
                || {
                    item.values_by_engine_count
                        .iter()
                        .map(|entry| format!("{}:{:.3}", entry.engine_count, entry.value))
                        .collect::<Vec<_>>()
                        .join("/")
                },
                |value| match value {
                    crate::domain::capabilities::RequirementTemplateValue::Quantity(value) => {
                        value.to_owned()
                    }
                    crate::domain::capabilities::RequirementTemplateValue::Scalar(value) => {
                        value.to_string()
                    }
                },
            );
            format!(
                "`{}` = {} [{}; {}]",
                item.id, value, item.severity, item.provenance.kind
            )
        })
        .collect::<Vec<_>>()
        .join("<br>");
    format!(
        "| `{}` | {} | {} | {} | {} |\n",
        template.id, template.version, template.category, parameter, items
    )
}

#[cfg(test)]
fn requirement_lists(manifest: &CapabilitiesManifest) -> (String, String) {
    (
        code_list(manifest.requirement_operators),
        code_list(manifest.requirement_severities),
    )
}

#[cfg(test)]
fn mission_notes() -> &'static str {
    "Energy-climb schedules use at least two strictly increasing altitude points, one speed \
     representation per point, and exactly one segment throttle setting. The solver uses fixed \
     midpoint steps and rejects nonpositive excess power rather than clamping it.\n\n\
     Legacy `climb` remains a low-fidelity constant-rate model capped at 50 m/s.\n\n\
     Power/thrust fractions on climb, energy_climb, cruise, loiter, and reserve constrain the \
     mission-power feasibility screen. Quasi-steady cruise/loiter fuel burn follows the \
     aerodynamic power required and is not scaled directly by throttle."
}

#[cfg(test)]
fn initial_state_markdown(fields: &[SegmentFieldCapability]) -> String {
    fields
        .iter()
        .map(field_markdown)
        .collect::<Vec<_>>()
        .join(", ")
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
        .map(field_markdown)
        .collect::<Vec<_>>()
        .join(", ");
    format!("| `{}` | {} |\n", segment.segment_type, fields)
}

#[cfg(test)]
fn field_markdown(field: &SegmentFieldCapability) -> String {
    let requirement = field_requirement(field.requirement);
    match field.alternative_group {
        Some(group) => format!("`{}` ({requirement}: {group})", field.name),
        None => format!("`{}` ({requirement})", field.name),
    }
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
        SegmentFieldRequirement::AtMostOne => "at_most_one",
    }
}

#[cfg(test)]
mod tests;
