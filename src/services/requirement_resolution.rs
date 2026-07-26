use std::collections::BTreeSet;

use serde_yaml::Value;

use crate::domain::capabilities::{
    REQUIREMENT_OPERATORS, REQUIREMENT_SEVERITIES, RequirementMetricCapability,
    RequirementTemplateCapability, RequirementTemplateItemCapability, RequirementTemplateValue,
    RequirementValueKind, requirement_metric, requirement_template,
};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::parse_quantity;
use crate::domain::schema::{
    RawRequirement, RawRequirementProvenance, Requirement, RequirementProvenance,
    RequirementTemplateReference, Requirements, RequirementsDocument,
};

struct MaterializedRequirement {
    raw: RawRequirement,
    provenance: RequirementProvenance,
}

pub(crate) fn resolve_requirements(document: RequirementsDocument) -> AexResult<Requirements> {
    let raw = document.requirements;
    ensure_unique_requirement_ids(&raw.items)?;
    let template = raw.template.clone();
    let items = materialize_requirements(template.as_ref(), raw.items)?
        .into_iter()
        .map(resolve_requirement)
        .collect::<AexResult<Vec<_>>>()?;
    Ok(Requirements {
        id: raw.id,
        template,
        items,
    })
}

pub(crate) fn validate_template_context(
    requirements: &Requirements,
    installed_engine_count: u32,
) -> AexResult<()> {
    let Some(reference) = &requirements.template else {
        return Ok(());
    };
    if let Some(template_engine_count) = reference.engine_count
        && template_engine_count != installed_engine_count
    {
        return Err(AexError::validation(
            "TEMPLATE_ENGINE_COUNT_MISMATCH",
            "requirements.template.engine_count",
            format!(
                "template selects {template_engine_count} engines but aircraft installs \
                 {installed_engine_count}"
            ),
        ));
    }
    Ok(())
}

fn ensure_unique_requirement_ids(items: &[RawRequirement]) -> AexResult<()> {
    let mut ids = BTreeSet::new();
    for (index, requirement) in items.iter().enumerate() {
        if !ids.insert(requirement.id.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_REQUIREMENT_ID",
                format!("requirements.items.{index}.id"),
                format!(
                    "requirement id {} is declared more than once",
                    requirement.id
                ),
            ));
        }
    }
    Ok(())
}

fn materialize_requirements(
    reference: Option<&RequirementTemplateReference>,
    items: Vec<RawRequirement>,
) -> AexResult<Vec<MaterializedRequirement>> {
    let Some(reference) = reference else {
        return Ok(items
            .into_iter()
            .map(|raw| MaterializedRequirement {
                provenance: custom_provenance(raw.provenance.as_ref()),
                raw,
            })
            .collect());
    };
    let template = requirement_template(&reference.id, reference.version).ok_or_else(|| {
        AexError::validation(
            "UNKNOWN_REQUIREMENT_TEMPLATE",
            "requirements.template",
            format!(
                "unknown template {} version {}",
                reference.id, reference.version
            ),
        )
    })?;
    validate_template_parameter(template, reference)?;
    let mut merged = template
        .items
        .iter()
        .map(|item| template_requirement(template, item, reference, &items))
        .collect::<AexResult<Vec<_>>>()?;
    merged.extend(
        items
            .into_iter()
            .filter(|raw| !template.items.iter().any(|item| item.id == raw.id))
            .map(|raw| MaterializedRequirement {
                provenance: custom_provenance(raw.provenance.as_ref()),
                raw,
            }),
    );
    Ok(merged)
}

fn validate_template_parameter(
    template: RequirementTemplateCapability,
    reference: &RequirementTemplateReference,
) -> AexResult<()> {
    match (template.parameter, reference.engine_count) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(AexError::validation(
            "UNSUPPORTED_TEMPLATE_PARAMETER",
            "requirements.template.engine_count",
            format!("template {} does not accept engine_count", template.id),
        )),
        (Some(parameter), None) if parameter.required => Err(AexError::validation(
            "MISSING_TEMPLATE_PARAMETER",
            format!("requirements.template.{}", parameter.name),
            format!("template {} requires {}", template.id, parameter.name),
        )),
        (Some(parameter), Some(value)) if !parameter.allowed_values.contains(&value) => {
            Err(AexError::validation(
                "INVALID_TEMPLATE_PARAMETER",
                format!("requirements.template.{}", parameter.name),
                format!(
                    "{} must be one of {:?}",
                    parameter.name, parameter.allowed_values
                ),
            ))
        }
        _ => Ok(()),
    }
}

fn template_requirement(
    template: RequirementTemplateCapability,
    item: &RequirementTemplateItemCapability,
    reference: &RequirementTemplateReference,
    supplied: &[RawRequirement],
) -> AexResult<MaterializedRequirement> {
    let override_item = supplied.iter().find(|raw| raw.id == item.id);
    if let Some(raw) = override_item {
        reject_immutable_template_fields(raw)?;
    }
    let value = override_item
        .and_then(|raw| raw.value.clone())
        .map(Ok)
        .unwrap_or_else(|| template_value(item, reference))?;
    let provenance = RequirementProvenance {
        kind: item.provenance.kind.to_owned(),
        source: item.provenance.source.to_owned(),
        citation: item.provenance.citation.map(str::to_owned),
        non_regulatory: item.provenance.non_regulatory,
        template_id: Some(template.id.to_owned()),
        template_version: Some(template.version),
    };
    Ok(MaterializedRequirement {
        raw: RawRequirement {
            id: item.id.to_owned(),
            metric: Some(item.metric.to_owned()),
            operator: Some(item.operator.to_owned()),
            value: Some(value),
            severity: override_item
                .and_then(|raw| raw.severity.clone())
                .or_else(|| Some(item.severity.to_owned())),
            weight: override_item.and_then(|raw| raw.weight).or(item.weight),
            provenance: None,
        },
        provenance,
    })
}

fn reject_immutable_template_fields(raw: &RawRequirement) -> AexResult<()> {
    for (field, present) in [
        ("metric", raw.metric.is_some()),
        ("operator", raw.operator.is_some()),
        ("provenance", raw.provenance.is_some()),
    ] {
        if present {
            return Err(AexError::validation(
                "TEMPLATE_REQUIREMENT_IMMUTABLE_FIELD",
                format!("requirements.items.{}.{}", raw.id, field),
                format!("template requirement {} cannot override {field}", raw.id),
            ));
        }
    }
    Ok(())
}

fn template_value(
    item: &RequirementTemplateItemCapability,
    reference: &RequirementTemplateReference,
) -> AexResult<Value> {
    if let Some(value) = item.value {
        return Ok(match value {
            RequirementTemplateValue::Quantity(value) => Value::String(value.to_owned()),
            RequirementTemplateValue::Scalar(value) => {
                serde_yaml::to_value(value).map_err(|source| AexError::Yaml {
                    path: "requirements.template".into(),
                    source,
                })?
            }
        });
    }
    let engine_count = reference.engine_count.ok_or_else(|| {
        AexError::validation(
            "MISSING_TEMPLATE_PARAMETER",
            "requirements.template.engine_count",
            "engine_count is required to select the template value",
        )
    })?;
    let value = item
        .values_by_engine_count
        .iter()
        .find(|entry| entry.engine_count == engine_count)
        .map(|entry| entry.value)
        .ok_or_else(|| {
            AexError::validation(
                "INVALID_TEMPLATE_PARAMETER",
                "requirements.template.engine_count",
                format!("no {} value exists for {engine_count} engines", item.id),
            )
        })?;
    serde_yaml::to_value(value).map_err(|source| AexError::Yaml {
        path: "requirements.template".into(),
        source,
    })
}

fn custom_provenance(raw: Option<&RawRequirementProvenance>) -> RequirementProvenance {
    raw.map_or_else(
        || RequirementProvenance {
            kind: "user_defined".to_owned(),
            source: "requirements document".to_owned(),
            citation: None,
            non_regulatory: true,
            template_id: None,
            template_version: None,
        },
        |provenance| RequirementProvenance {
            kind: provenance.kind.clone(),
            source: provenance.source.clone(),
            citation: provenance.citation.clone(),
            non_regulatory: provenance.non_regulatory,
            template_id: None,
            template_version: None,
        },
    )
}

fn resolve_requirement(materialized: MaterializedRequirement) -> AexResult<Requirement> {
    let raw = materialized.raw;
    let metric = required_string(raw.metric, &raw.id, "metric")?;
    let capability = requirement_metric(&metric).ok_or_else(|| {
        AexError::validation(
            "UNSUPPORTED_REQUIREMENT_METRIC",
            &metric,
            "metric is not implemented",
        )
    })?;
    if !capability.bindable {
        let replacement = capability.replacement.unwrap_or("an achieved metric");
        return Err(AexError::validation(
            "DECLARED_METRIC_NOT_BINDABLE",
            format!("requirements.items.{}.metric", raw.id),
            format!("{metric} is a declared input; bind the requirement to {replacement}"),
        ));
    }
    let value = raw.value.ok_or_else(|| missing_field(&raw.id, "value"))?;
    let operator = required_string(raw.operator, &raw.id, "operator")?;
    let severity = required_string(raw.severity, &raw.id, "severity")?;
    let (required, unit) = requirement_value(capability, &value)?;
    if !REQUIREMENT_OPERATORS.contains(&operator.as_str()) {
        return Err(AexError::validation(
            "INVALID_REQUIREMENT_OPERATOR",
            format!("requirements.items.{}.operator", raw.id),
            format!("unsupported operator {operator}"),
        ));
    }
    if !REQUIREMENT_SEVERITIES.contains(&severity.as_str()) {
        return Err(AexError::validation(
            "INVALID_REQUIREMENT_SEVERITY",
            format!("requirements.items.{}.severity", raw.id),
            format!("unsupported severity {severity}"),
        ));
    }
    Ok(Requirement {
        id: raw.id,
        metric,
        operator,
        required,
        unit,
        severity,
        weight: raw.weight,
        provenance: materialized.provenance,
    })
}

fn required_string(value: Option<String>, id: &str, field: &str) -> AexResult<String> {
    value.ok_or_else(|| missing_field(id, field))
}

fn missing_field(id: &str, field: &str) -> AexError {
    AexError::validation(
        "MISSING_REQUIREMENT_FIELD",
        format!("requirements.items.{id}.{field}"),
        format!("custom requirement {id} must declare {field}"),
    )
}

fn requirement_value(
    capability: RequirementMetricCapability,
    value: &Value,
) -> AexResult<(f64, String)> {
    match capability.value_kind.dimension() {
        Some(dimension) => {
            let raw = value.as_str().ok_or_else(|| {
                AexError::validation(
                    "AMBIGUOUS_UNITLESS_VALUE",
                    capability.id,
                    "physical requirements require a unit",
                )
            })?;
            Ok((
                parse_quantity(raw, dimension)?,
                capability.canonical_unit.to_owned(),
            ))
        }
        None if capability.value_kind == RequirementValueKind::Scalar => value
            .as_f64()
            .map(|number| (number, capability.canonical_unit.to_owned()))
            .ok_or_else(|| {
                AexError::validation(
                    "INVALID_REQUIREMENT_VALUE",
                    capability.id,
                    "expected number",
                )
            }),
        None => Err(AexError::validation(
            "INVALID_REQUIREMENT_VALUE_KIND",
            capability.id,
            "physical requirement metric has no quantity dimension",
        )),
    }
}

#[cfg(test)]
mod tests;
