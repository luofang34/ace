use super::*;

const ROLES: PayloadRangeRoles<'static> = PayloadRangeRoles {
    distance: "mission.distance",
    remaining: "mission.remaining_fuel",
    maximum_takeoff_mass: "weight.mtow",
    maximum_payload: "weight.maximum_payload",
};

fn c172() -> serde_json::Value {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/vehicle.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn compute(model: &serde_json::Value) -> AexResult<PayloadRange> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vehicle.json");
    std::fs::write(&path, serde_json::to_vec(model).unwrap()).unwrap();
    compute_blocking(&path, &ROLES)
}

#[test]
fn ambiguous_fuel_items_are_rejected_before_any_mission() {
    let mut model = c172();
    let fuel = model["mass_items"]["fuel"].clone();
    model["mass_items"]["auxiliary_fuel"] = fuel;
    let Err(AexError::Validation { code, .. }) = compute(&model) else {
        panic!("two fuel items must not be resolved by guessing");
    };
    assert_eq!(code, "INVALID_PAYLOAD_RANGE_LOADING");
}

#[test]
fn a_mission_failing_for_a_non_fuel_reason_is_an_error_not_a_range() {
    let mut model = c172();
    // A 2 km path to cruise altitude needs more thrust than the engine has at any loading.
    model["parameters"]["phase.climb.path_distance"]["value"]["value"] = 2_000.0.into();
    let Err(error) = compute(&model) else {
        panic!("a thrust-limited climb must not be reported as a fuel-limited range");
    };
    let message = error.to_string();
    assert!(message.contains("other than fuel"), "{message}");
    assert!(message.contains("climb"), "{message}");
}
