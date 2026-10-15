use serde_json::json;

use crate::test_support::example_scenario;

use super::{FileRunStore, PersistRunRequest, RunModelUsage, content_hash, manifest};

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

#[test]
fn identical_runs_in_one_second_are_idempotent_and_conflicts_still_fail() {
    let resolved = example_scenario("c172").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = FileRunStore::new(directory.path().to_path_buf());
    let input = json!({"scenario": "c172"});
    let result = json!({"value": 1.0});
    let request = || PersistRunRequest {
        scenario: &resolved,
        analysis: "point",
        original_input: &input,
        result: &result,
        warnings: &[],
        seed: 7,
        artifacts: &[],
        model_usage: RunModelUsage::default(),
    };
    let timestamp = chrono::Utc::now();
    let first = store.persist_at_blocking(request(), timestamp).unwrap();
    let second = store.persist_at_blocking(request(), timestamp).unwrap();
    assert_eq!(first.run_id, second.run_id);
    assert_eq!(first.manifest.content_hash, second.manifest.content_hash);
    let runs = std::fs::read_dir(directory.path()).unwrap().count();
    assert_eq!(runs, 1);

    let path = first.directory.join("manifest.json");
    let mut stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stored["content_hash"] = json!("0".repeat(64));
    std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
    let error = store.persist_at_blocking(request(), timestamp).unwrap_err();
    assert!(
        error.to_string().contains("RUN_ID_COLLISION")
            || format!("{error:?}").contains("RUN_ID_COLLISION")
    );
}
