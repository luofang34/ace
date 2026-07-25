#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use crate::domain::diagnostic::{Diagnostic, Severity};

use super::{WarningCode, enforce_strict, policy_markdown, warning_policy};

#[test]
fn policy_codes_are_unique_and_round_trip() {
    let policy = warning_policy();
    let codes = policy
        .iter()
        .map(|entry| entry.code.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(codes.len(), policy.len());
    assert!(
        policy
            .iter()
            .all(|entry| WarningCode::from_wire(entry.code.as_str()) == Some(entry.code))
    );
    assert_eq!(
        serde_json::to_value(policy).expect("policy must serialize")[0]["code"],
        "AGENT_ASSUMPTION"
    );
}

#[test]
fn generated_reference_matches_the_runtime_policy() {
    assert_eq!(
        policy_markdown(),
        include_str!("../../../docs/strict-warning-policy.md")
    );
}

#[test]
fn strict_enforcement_promotes_registered_codes_only() -> Result<(), Box<dyn std::error::Error>> {
    let promoted = Diagnostic::warning(
        WarningCode::ModelExtrapolation,
        "outside model range",
        "condition.mach",
    );
    assert!(enforce_strict(true, &[promoted]).is_err());

    let unknown = Diagnostic {
        code: "FUTURE_STORED_WARNING".to_owned(),
        severity: Severity::Warning,
        message: "stored compatibility diagnostic".to_owned(),
        path: None,
        context: serde_json::Value::Object(serde_json::Map::new()),
    };
    enforce_strict(true, &[unknown])?;
    Ok(())
}
