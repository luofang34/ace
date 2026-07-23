use crate::test_support::example_scenario;

use super::PointAnalyzer;

#[test]
fn both_reference_aircraft_have_finite_service_ceilings() {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name);
        assert!(scenario.is_ok());
        if let Ok(resolved) = scenario {
            let summary = PointAnalyzer::new(resolved).summary();
            assert!(summary.is_ok());
            if let Ok(result) = summary {
                assert!(result.service_ceiling_m.is_finite());
                assert!(result.service_ceiling_m > 0.0);
                assert!(result.absolute_ceiling_m >= result.service_ceiling_m);
            }
        }
    }
}
