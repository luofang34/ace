use rmcp::ErrorData;
use rmcp::model::CallToolResult;
use serde_json::{Map, Value, json};

use crate::domain::diagnostic::{AexError, ErrorDetail};
use crate::domain::validity::{ModelDomainViolation, ValidityVariable};

const DOMAIN_ERROR_MARKER: &str = "_aex_domain_error";

pub(super) fn mcp_error(source: AexError) -> ErrorData {
    let detail = source.detail();
    let message = detail.message.clone();
    let mut envelope = domain_envelope(&source, detail);
    if let Value::Object(fields) = &mut envelope {
        fields.insert(DOMAIN_ERROR_MARKER.to_owned(), Value::Bool(true));
    }
    ErrorData::internal_error(message, Some(envelope))
}

pub(super) fn into_tool_result(mut error: ErrorData) -> Result<CallToolResult, ErrorData> {
    let is_domain = error
        .data
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|fields| fields.get(DOMAIN_ERROR_MARKER))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !is_domain {
        return Err(error);
    }
    let mut envelope = error.data.take().unwrap_or_else(empty_object);
    if let Value::Object(fields) = &mut envelope {
        fields.remove(DOMAIN_ERROR_MARKER);
    }
    Ok(CallToolResult::structured_error(envelope))
}

fn domain_envelope(source: &AexError, detail: ErrorDetail) -> Value {
    let diagnostics = match source {
        AexError::ModelDomainUnsupported { violations, .. } => violations
            .iter()
            .map(|violation| violation_diagnostic(&detail.code, violation))
            .collect(),
        _ => vec![json!({
            "code": detail.code,
            "severity": "error",
            "message": detail.message,
            "path": detail.path,
            "violating_path": detail.path,
            "context": detail.context,
        })],
    };
    let first_violation = match source {
        AexError::ModelDomainUnsupported { violations, .. } => violations.first(),
        _ => None,
    };
    json!({
        "status": "error",
        "code": detail.code,
        "message": detail.message,
        "path": detail.path,
        "violating_path": detail.path,
        "valid_range": first_violation.map(valid_range),
        "suggested_override": first_violation.and_then(suggested_override),
        "diagnostics": diagnostics,
        "context": detail.context,
    })
}

fn violation_diagnostic(code: &str, violation: &ModelDomainViolation) -> Value {
    json!({
        "code": code,
        "severity": "error",
        "message": format!(
            "{} is outside the {} validity domain",
            violation.path, violation.model_id
        ),
        "path": violation.path,
        "violating_path": violation.path,
        "valid_range": valid_range(violation),
        "suggested_override": suggested_override(violation),
        "context": violation,
    })
}

fn valid_range(violation: &ModelDomainViolation) -> Value {
    json!({
        "model_id": violation.model_id,
        "variable": violation.variable,
        "minimum": violation.minimum,
        "maximum": violation.maximum,
        "minimum_inclusive": violation.minimum_inclusive,
        "maximum_inclusive": violation.maximum_inclusive,
        "unit": violation.bound_unit,
        "basis": violation.basis,
    })
}

fn suggested_override(violation: &ModelDomainViolation) -> Option<Value> {
    if !path_directly_represents(violation.variable, &violation.path) {
        return None;
    }
    let bound = if violation
        .minimum
        .is_some_and(|minimum| violation.declared_value < minimum)
        && violation.minimum_inclusive
    {
        violation.minimum
    } else if violation
        .maximum
        .is_some_and(|maximum| violation.declared_value > maximum)
        && violation.maximum_inclusive
    {
        violation.maximum
    } else {
        None
    }?;
    let value = if violation.bound_unit == "1" {
        json!(bound)
    } else {
        json!(format!("{bound} {}", violation.bound_unit))
    };
    Some(json!({
        "path": violation.path,
        "value": value,
        "reason": "nearest inclusive model-domain bound",
    }))
}

fn path_directly_represents(variable: ValidityVariable, path: &str) -> bool {
    let field = path.rsplit('.').next().unwrap_or(path);
    match variable {
        ValidityVariable::Altitude => field.ends_with("altitude"),
        ValidityVariable::Mach => field.ends_with("mach"),
        ValidityVariable::TrueAirspeed => {
            field.ends_with("true_airspeed") || field.ends_with("operating_speed")
        }
        ValidityVariable::Mass => field.ends_with("mass"),
        ValidityVariable::Throttle => field.ends_with("throttle"),
        ValidityVariable::AspectRatio => field == "aspect_ratio",
        ValidityVariable::LoadFactor => field.ends_with("load_factor"),
    }
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

#[cfg(test)]
mod tests;
