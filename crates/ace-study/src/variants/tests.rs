use super::*;
use ace_model::Unit;

fn matrix() -> VariantMatrix {
    let axes = ["duration", "speed", "fuel"]
        .into_iter()
        .map(|axis| {
            let choices = ["low", "high"]
                .into_iter()
                .enumerate()
                .map(|(i, name)| {
                    (
                        name.into(),
                        VariantOption {
                            edits: vec![],
                            updates: BTreeMap::from([(
                                axis.into(),
                                Quantity::new(i as f64, Unit::Ratio).unwrap(),
                            )]),
                        },
                    )
                })
                .collect();
            (axis.into(), choices)
        })
        .collect();
    VariantMatrix {
        axes,
        exclusions: vec![],
        max_combinations: 8,
    }
}

#[test]
fn bounded_matrix_is_deterministic_complete_and_rejects_silent_budget_truncation() {
    let mut matrix = matrix();
    let cases = matrix.expand().unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(
        cases
            .iter()
            .map(|case| &case.id)
            .collect::<BTreeSet<_>>()
            .len(),
        8
    );
    for a in ["low", "high"] {
        for b in ["low", "high"] {
            for c in ["low", "high"] {
                assert!(cases.iter().any(|case| case.choices["duration"] == a
                    && case.choices["speed"] == b
                    && case.choices["fuel"] == c));
            }
        }
    }
    let reopened: VariantMatrix =
        serde_json::from_slice(&serde_json::to_vec(&matrix).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(cases).unwrap(),
        serde_json::to_value(reopened.expand().unwrap()).unwrap()
    );
    matrix.max_combinations = 7;
    assert!(matrix.expand().is_err());
}

#[test]
fn exclusions_are_explicit_and_unknown_or_conflicting_choices_fail() {
    let mut matrix = matrix();
    matrix.exclusions.push(BTreeMap::from([
        ("duration".into(), "high".into()),
        ("speed".into(), "low".into()),
    ]));
    let cases = matrix.expand().unwrap();
    assert_eq!(cases.len(), 6);
    assert!(
        !cases
            .iter()
            .any(|case| case.choices["duration"] == "high" && case.choices["speed"] == "low")
    );
    matrix
        .exclusions
        .push(BTreeMap::from([("missing".into(), "low".into())]));
    assert!(matrix.expand().is_err());
    matrix.exclusions.clear();
    matrix
        .axes
        .get_mut("fuel")
        .unwrap()
        .get_mut("low")
        .unwrap()
        .updates
        .insert("duration".into(), Quantity::new(1.0, Unit::Ratio).unwrap());
    assert!(matrix.expand().is_err());
}
