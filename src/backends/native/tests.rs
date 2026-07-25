use crate::models::mission::MissionSimulator;
use crate::test_support::example_scenario;

use super::failed_constraints;

#[test]
fn fuel_failure_constraint_labels_are_independent() -> Result<(), Box<dyn std::error::Error>> {
    let mut capacity = MissionSimulator::new(example_scenario("c172")?).simulate()?;
    capacity.fuel_capacity_violation = true;
    let capacity_failures = failed_constraints(&[], &capacity);
    assert!(
        capacity_failures
            .iter()
            .any(|constraint| constraint == "fuel_capacity")
    );
    assert!(
        !capacity_failures
            .iter()
            .any(|constraint| constraint == "fuel_exhausted")
    );

    let exhaustion = MissionSimulator::new(example_scenario("x15")?).simulate()?;
    let exhaustion_failures = failed_constraints(&[], &exhaustion);
    assert!(
        exhaustion_failures
            .iter()
            .any(|constraint| constraint == "fuel_exhausted")
    );
    assert!(
        !exhaustion_failures
            .iter()
            .any(|constraint| constraint == "fuel_capacity")
    );
    Ok(())
}
