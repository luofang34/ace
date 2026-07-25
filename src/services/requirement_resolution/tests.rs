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
