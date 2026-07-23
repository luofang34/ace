use serde_json::json;

use crate::test_support::example_scenario;

use super::{PersistRunRequest, content_hash};

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
        };
        let first = content_hash(&request);
        let second = content_hash(&request);
        assert_eq!(first.ok(), second.ok());
    }
}
