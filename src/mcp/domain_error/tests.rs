#![allow(clippy::expect_used, clippy::panic)]

use rmcp::ErrorData;
use serde_json::Value;

use crate::domain::diagnostic::AexError;
use crate::domain::validity::{ModelDomainViolation, ValidityBasis, ValidityVariable};

use super::{domain_envelope, into_tool_result, mcp_error};

#[test]
fn model_domain_envelope_retains_ranges_and_computable_hints() {
    let error = AexError::ModelDomainUnsupported {
        message: "two violations".to_owned(),
        violations: vec![
            violation(
                "mission.segments.cruise.altitude",
                ValidityVariable::Altitude,
                23_000.0,
                "m",
                Some(-2_000.0),
                Some(20_000.0),
            ),
            violation(
                "condition.true_airspeed",
                ValidityVariable::Mach,
                1.2,
                "1",
                Some(0.0),
                Some(0.9),
            ),
        ],
    };
    let envelope = domain_envelope(&error, error.detail());
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["valid_range"]["maximum"], 20_000.0);
    assert_eq!(
        envelope["suggested_override"]["value"],
        Value::String("20000 m".to_owned())
    );
    assert_eq!(envelope["diagnostics"].as_array().map(Vec::len), Some(2));
    assert!(envelope["diagnostics"][1]["suggested_override"].is_null());
}

#[test]
fn only_marked_domain_errors_become_tool_results() {
    let domain = into_tool_result(mcp_error(AexError::analysis("TEST_DOMAIN", "failure")))
        .expect("domain error should become a tool result");
    assert_eq!(domain.is_error, Some(true));
    assert_eq!(
        domain
            .structured_content
            .as_ref()
            .map(|value| &value["code"]),
        Some(&Value::String("TEST_DOMAIN".to_owned()))
    );

    let protocol = ErrorData::invalid_params("malformed", None);
    assert!(into_tool_result(protocol).is_err());
}

fn violation(
    path: &str,
    variable: ValidityVariable,
    declared_value: f64,
    unit: &str,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> ModelDomainViolation {
    ModelDomainViolation {
        path: path.to_owned(),
        model_id: "test.model".to_owned(),
        variable,
        declared_value,
        declared_unit: unit.to_owned(),
        minimum,
        maximum,
        minimum_inclusive: true,
        maximum_inclusive: true,
        bound_unit: unit.to_owned(),
        basis: ValidityBasis::PublishedSpecification,
    }
}
