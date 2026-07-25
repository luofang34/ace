use crate::domain::diagnostic::AexError;
use crate::domain::schema::RequirementsDocument;

use super::{resolve_requirements, validate_template_context};

#[test]
fn duplicate_requirement_ids_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let document: RequirementsDocument = serde_yaml::from_str(
        r#"
schema_version: 1
requirements:
  id: duplicate_requirements
  items:
    - id: range
      metric: mission.completed_distance
      operator: ge
      value: 350 nmi
      severity: hard
    - id: range
      metric: mission.completed_distance
      operator: ge
      value: 10000 nmi
      severity: hard
"#,
    )?;

    assert!(matches!(
        resolve_requirements(document),
        Err(AexError::Validation {
            code: "DUPLICATE_REQUIREMENT_ID",
            path,
            ..
        }) if path == "requirements.items.1.id"
    ));
    Ok(())
}

#[test]
fn declared_cruise_metrics_name_the_achieved_replacement() -> Result<(), Box<dyn std::error::Error>>
{
    for (metric, replacement) in [
        (
            "performance.cruise_mach",
            "performance.achieved_cruise_mach",
        ),
        (
            "performance.cruise_true_airspeed",
            "performance.achieved_cruise_true_airspeed",
        ),
    ] {
        let source = format!(
            r#"
schema_version: 1
requirements:
  id: declared_metric
  items:
    - id: cruise
      metric: {metric}
      operator: ge
      value: 1
      severity: hard
"#
        );
        let document: RequirementsDocument = serde_yaml::from_str(&source)?;
        assert!(matches!(
            resolve_requirements(document),
            Err(AexError::Validation {
                code: "DECLARED_METRIC_NOT_BINDABLE",
                path,
                message,
            }) if path == "requirements.items.cruise.metric"
                && message.contains(replacement)
        ));
    }
    Ok(())
}

#[test]
fn light_template_resolves_defaults_overrides_and_custom_items()
-> Result<(), Box<dyn std::error::Error>> {
    let document: RequirementsDocument = serde_yaml::from_str(
        r#"
schema_version: 1
requirements:
  id: light
  template:
    id: light_aircraft_conceptual
    version: 1
  items:
    - id: stall_speed_landing
      value: 50 kt
      severity: hard
      weight: 0.7
    - id: payload
      metric: mission.payload_mass
      operator: ge
      value: 200 kg
      severity: hard
"#,
    )?;
    let resolved = resolve_requirements(document)?;
    assert_eq!(resolved.items.len(), 6);
    let stall = resolved
        .items
        .iter()
        .find(|item| item.id == "stall_speed_landing")
        .ok_or("missing template stall requirement")?;
    assert!((stall.required - 25.722_222_222).abs() < 1.0e-6);
    assert_eq!(stall.severity, "hard");
    assert_eq!(stall.weight, Some(0.7));
    assert_eq!(
        stall.provenance.template_id.as_deref(),
        Some("light_aircraft_conceptual")
    );
    assert!(stall.provenance.non_regulatory);
    assert_eq!(
        resolved.items.last().map(|item| item.id.as_str()),
        Some("payload")
    );
    Ok(())
}

#[test]
fn transport_template_selects_oei_gradient_by_engine_count()
-> Result<(), Box<dyn std::error::Error>> {
    for (engine_count, required) in [(2, 0.024), (3, 0.027), (4, 0.030)] {
        let source = format!(
            r#"
schema_version: 1
requirements:
  id: transport
  template:
    id: transport_conceptual
    version: 1
    engine_count: {engine_count}
"#
        );
        let resolved = resolve_requirements(serde_yaml::from_str(&source)?)?;
        let oei = resolved
            .items
            .iter()
            .find(|item| item.id == "oei_second_segment_climb_gradient")
            .ok_or("missing OEI requirement")?;
        assert_eq!(oei.required, required);
        assert_eq!(oei.provenance.kind, "regulatory_derived");
        assert!(
            oei.provenance
                .citation
                .as_deref()
                .is_some_and(|citation| citation.contains("25.121"))
        );
        assert!(!oei.provenance.non_regulatory);
        validate_template_context(&resolved, engine_count)?;
        if engine_count != 2 {
            assert!(matches!(
                validate_template_context(&resolved, 2),
                Err(AexError::Validation {
                    code: "TEMPLATE_ENGINE_COUNT_MISMATCH",
                    path,
                    ..
                }) if path == "requirements.template.engine_count"
            ));
        }
    }
    Ok(())
}

#[test]
fn template_identity_fields_are_immutable() -> Result<(), Box<dyn std::error::Error>> {
    for (field, declaration) in [
        ("metric", "metric: mission.payload_mass"),
        ("operator", "operator: ge"),
        (
            "provenance",
            "provenance:\n        kind: user_defined\n        source: test",
        ),
    ] {
        let source = format!(
            r#"
schema_version: 1
requirements:
  id: immutable
  template:
    id: light_aircraft_conceptual
    version: 1
  items:
    - id: stall_speed_landing
      {declaration}
"#
        );
        assert!(matches!(
            resolve_requirements(serde_yaml::from_str(&source)?),
            Err(AexError::Validation {
                code: "TEMPLATE_REQUIREMENT_IMMUTABLE_FIELD",
                path,
                ..
            }) if path == format!("requirements.items.stall_speed_landing.{field}")
        ));
    }
    Ok(())
}

#[test]
fn template_parameters_are_explicit_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
    for (template, expected_code) in [
        (
            "id: transport_conceptual\n    version: 1",
            "MISSING_TEMPLATE_PARAMETER",
        ),
        (
            "id: transport_conceptual\n    version: 1\n    engine_count: 5",
            "INVALID_TEMPLATE_PARAMETER",
        ),
        (
            "id: light_aircraft_conceptual\n    version: 1\n    engine_count: 2",
            "UNSUPPORTED_TEMPLATE_PARAMETER",
        ),
    ] {
        let source = format!(
            "schema_version: 1\nrequirements:\n  id: invalid\n  template:\n    {template}\n"
        );
        assert!(matches!(
            resolve_requirements(serde_yaml::from_str(&source)?),
            Err(AexError::Validation { code, .. }) if code == expected_code
        ));
    }
    Ok(())
}

#[test]
fn legacy_custom_requirements_gain_additive_provenance() -> Result<(), Box<dyn std::error::Error>> {
    let document: RequirementsDocument = serde_yaml::from_str(
        r#"
schema_version: 1
requirements:
  id: legacy
  items:
    - id: payload
      metric: mission.payload_mass
      operator: ge
      value: 1 kg
      severity: hard
"#,
    )?;
    let resolved = resolve_requirements(document)?;
    assert!(resolved.template.is_none());
    assert_eq!(resolved.items[0].provenance.kind, "user_defined");
    assert!(resolved.items[0].provenance.non_regulatory);
    Ok(())
}
