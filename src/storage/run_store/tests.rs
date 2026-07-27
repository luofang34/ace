use serde_json::json;

use crate::test_support::example_scenario;

use super::{PersistRunRequest, RunModelUsage, content_hash, manifest};

#[test]
fn content_hash_is_stable() {
    let scenario = example_scenario("c172");
    assert!(scenario.is_ok());
    if let Ok(resolved) = scenario {
        let input = json!({"scenario": "c172"});
        let result = json!({"value": 1.0});
        let request = PersistRunRequest {
            scenario: &resolved,
            analysis: "test",
            original_input: &input,
            result: &result,
            warnings: &[],
            seed: 7,
            artifacts: &[],
            model_usage: RunModelUsage::default(),
        };
        let first = content_hash(&request);
        let second = content_hash(&request);
        assert_eq!(first.ok(), second.ok());
    }
}

#[test]
fn manifest_attributes_only_models_executed_by_the_analysis() {
    let scenario = example_scenario("c172");
    assert!(scenario.is_ok());
    if let Ok(resolved) = scenario {
        let input = json!({"scenario": "c172"});
        let result = json!({"value": 1.0});
        for (analysis, has_stability) in [
            ("point", false),
            ("mission", true),
            ("sweep", true),
            ("plot", true),
        ] {
            let request = PersistRunRequest {
                scenario: &resolved,
                analysis,
                original_input: &input,
                result: &result,
                warnings: &[],
                seed: 7,
                artifacts: &[],
                model_usage: if has_stability {
                    RunModelUsage::with_mass_properties()
                } else {
                    RunModelUsage::default()
                },
            };
            let stored = manifest(&request, "run", "hash", "time".to_owned());
            assert!(
                stored
                    .models
                    .iter()
                    .any(|model| { model.model_id == "mass.component_buildup" })
            );
            assert_eq!(
                stored
                    .models
                    .iter()
                    .any(|model| { model.model_id == "stability.native_mass_properties" }),
                has_stability
            );
        }
    }
}
