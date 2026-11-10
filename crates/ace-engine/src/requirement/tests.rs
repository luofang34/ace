use super::*;
use crate::{Registry, Session};
use ace_model::{Parameter, Priority, Source, Unit};

fn source() -> Source {
    Source {
        kind: "user".into(),
        reference: "test fixture".into(),
        revision: None,
    }
}

fn status(
    relation: Relation,
    actual_nmi: f64,
    target_nmi: f64,
) -> crate::EngineResult<MarginStatus> {
    let unit = Unit::NauticalMile;
    let flown = Parameter {
        unit,
        value: Some(Quantity::new(actual_nmi, unit)?),
        expression: None,
        source: source(),
        owner: "design".into(),
        revision: 0,
        uncertainty: None,
        lower: None,
        upper: None,
    };
    let requirement = Requirement {
        name: "Range".into(),
        metric: "flown".into(),
        relation,
        target: Quantity::new(target_nmi, unit)?,
        tolerance: None,
        priority: Priority::Hard,
        source: source(),
        condition: BTreeMap::new(),
        geometry: Vec::new(),
        confidence: None,
    };
    let mut model: VehicleModel = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": "test", "name": "Test vehicle", "domain": "generic",
        "parameters": {}
    }))
    .map_err(|error| ace_model::ModelError::invalid("fixture", error.to_string()))?;
    model.parameters.insert("flown".into(), flown);
    model.requirements.insert("range".into(), requirement);
    let mut session = Session::new(model, Registry::default())?;
    let snapshot = session.run()?;
    Ok(evaluate_requirements(session.model(), &snapshot)?["range"].status)
}

#[test]
fn a_target_met_to_rounding_is_active_not_violated() -> crate::EngineResult<()> {
    // Legs of a 600 nmi mission summed in metres and read back in nautical miles.
    let closed = 600.0 - 9.0e-13;
    assert!(closed < 600.0);
    assert_eq!(status(Relation::Ge, closed, 600.0)?, MarginStatus::Warn);
    assert_eq!(
        status(Relation::Le, 600.0 + 9.0e-13, 600.0)?,
        MarginStatus::Warn
    );
    assert_eq!(status(Relation::Ge, 600.0, 600.0)?, MarginStatus::Warn);
    Ok(())
}

#[test]
fn a_shortfall_larger_than_rounding_still_fails() -> crate::EngineResult<()> {
    assert_eq!(status(Relation::Ge, 599.9999, 600.0)?, MarginStatus::Fail);
    assert_eq!(
        status(Relation::Ge, 600.0 - 1.0e-6, 600.0)?,
        MarginStatus::Fail
    );
    assert_eq!(status(Relation::Le, 600.0001, 600.0)?, MarginStatus::Fail);
    assert_eq!(status(Relation::Ge, 660.0, 600.0)?, MarginStatus::Pass);
    Ok(())
}
