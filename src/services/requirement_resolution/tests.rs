use crate::domain::diagnostic::AexError;
use crate::domain::schema::RequirementsDocument;

use super::resolve_requirements;

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
