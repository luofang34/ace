use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RequirementsDocument {
    pub(crate) schema_version: u32,
    pub(crate) requirements: RawRequirements,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawRequirements {
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) template: Option<RequirementTemplateReference>,
    #[serde(default)]
    pub(crate) items: Vec<RawRequirement>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RequirementTemplateReference {
    pub(crate) id: String,
    pub(crate) version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) engine_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawRequirement {
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) metric: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) severity: Option<String>,
    pub(crate) weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provenance: Option<RawRequirementProvenance>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawRequirementProvenance {
    pub(crate) kind: String,
    pub(crate) source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) citation: Option<String>,
    #[serde(default)]
    pub(crate) non_regulatory: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Requirements {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) template: Option<RequirementTemplateReference>,
    pub(crate) items: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Requirement {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) operator: String,
    pub(crate) required: f64,
    pub(crate) unit: String,
    pub(crate) severity: String,
    pub(crate) weight: Option<f64>,
    pub(crate) provenance: RequirementProvenance,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RequirementProvenance {
    pub(crate) kind: String,
    pub(crate) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) citation: Option<String>,
    pub(crate) non_regulatory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) template_version: Option<u32>,
}
