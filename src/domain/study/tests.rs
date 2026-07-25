use crate::domain::diagnostic::AexError;

use super::{StudyDocument, study_digest, validate_study_document};

const MINIMAL_STUDY: &str = r#"
schema_version: 1
study:
  id: wing-trade
  name: Wing trade
  baseline:
    scenario_path: scenario.yaml
  variables:
    - id: wing_area
      path: aircraft.geometry.wing.area
      kind: continuous
      values: ["15 m^2", "16 m^2"]
  objectives:
    - id: minimize_fuel
      metric: mission.total_fuel
      direction: minimize
"#;

#[test]
fn omitted_policy_fields_use_stable_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let study: StudyDocument = serde_yaml::from_str(MINIMAL_STUDY)?;

    validate_study_document(&study)?;
    assert_eq!(study.study.analysis.screening_backend, "native");
    assert_eq!(study.study.analysis.refinement_candidate_limit, 3);
    assert_eq!(study.study.search.strategy, "grid");
    assert_eq!(study.study.search.max_evaluations, 256);
    assert_eq!(study.study.search.population, 24);
    assert_eq!(study.study.search.generations, 12);
    assert_eq!(study.study.search.mutation_rate, 0.15);
    assert_eq!(study.study.search.seed, 0);

    let round_trip: StudyDocument = serde_yaml::from_str(&serde_yaml::to_string(&study)?)?;
    assert_eq!(study_digest(&study)?, study_digest(&round_trip)?);
    Ok(())
}

#[test]
fn digest_is_canonical_and_change_sensitive() -> Result<(), Box<dyn std::error::Error>> {
    let first: StudyDocument = serde_yaml::from_str(MINIMAL_STUDY)?;
    let reordered: StudyDocument = serde_yaml::from_str(
        r#"
study:
  objectives:
    - direction: minimize
      metric: mission.total_fuel
      id: minimize_fuel
  variables:
    - values: ["15 m^2", "16 m^2"]
      kind: continuous
      path: aircraft.geometry.wing.area
      id: wing_area
  baseline: { scenario_path: scenario.yaml }
  name: Wing trade
  id: wing-trade
schema_version: 1
"#,
    )?;
    let changed: StudyDocument =
        serde_yaml::from_str(&MINIMAL_STUDY.replace("direction: minimize", "direction: maximize"))?;

    assert_eq!(study_digest(&first)?, study_digest(&reordered)?);
    assert_ne!(study_digest(&first)?, study_digest(&changed)?);
    Ok(())
}

#[test]
fn invalid_baselines_and_objectives_have_stable_codes() {
    let absolute = MINIMAL_STUDY.replace("scenario.yaml", "/tmp/scenario.yaml");
    let absolute: Result<StudyDocument, _> = serde_yaml::from_str(&absolute);
    assert!(absolute.is_ok());
    if let Ok(document) = absolute {
        assert!(matches!(
            validate_study_document(&document),
            Err(AexError::Validation {
                code: "ABSOLUTE_STUDY_REFERENCE",
                ..
            })
        ));
    }

    let invalid_weight = MINIMAL_STUDY.replace(
        "direction: minimize",
        "direction: minimize\n      weight: 0",
    );
    let invalid_weight: Result<StudyDocument, _> = serde_yaml::from_str(&invalid_weight);
    assert!(invalid_weight.is_ok());
    if let Ok(document) = invalid_weight {
        assert!(matches!(
            validate_study_document(&document),
            Err(AexError::Validation {
                code: "INVALID_STUDY_OBJECTIVE",
                ..
            })
        ));
    }
}

#[test]
fn duplicate_variables_and_invalid_integer_values_are_rejected() {
    let duplicate = MINIMAL_STUDY.replace(
        "  objectives:",
        r#"    - id: other_area
      path: aircraft.geometry.wing.area
      kind: continuous
      values: ["17 m^2"]
  objectives:"#,
    );
    let duplicate: Result<StudyDocument, _> = serde_yaml::from_str(&duplicate);
    assert!(duplicate.is_ok());
    if let Ok(document) = duplicate {
        assert!(matches!(
            validate_study_document(&document),
            Err(AexError::Validation {
                code: "INVALID_STUDY_VARIABLE",
                ..
            })
        ));
    }

    let non_integer = MINIMAL_STUDY
        .replace("kind: continuous", "kind: integer")
        .replace("[\"15 m^2\", \"16 m^2\"]", "[\"1\", \"1.5\"]");
    let non_integer: Result<StudyDocument, _> = serde_yaml::from_str(&non_integer);
    assert!(non_integer.is_ok());
    if let Ok(document) = non_integer {
        assert!(matches!(
            validate_study_document(&document),
            Err(AexError::Validation {
                code: "INVALID_INTEGER_STUDY_VALUE",
                ..
            })
        ));
    }
}
