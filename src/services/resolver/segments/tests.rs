#![allow(clippy::expect_used)]

use crate::domain::schema::RawMissionSegment;

use super::resolve_segment;

fn fixed_fuel() -> RawMissionSegment {
    RawMissionSegment {
        id: "segment".to_owned(),
        kind: "fixed_fuel".to_owned(),
        duration: None,
        distance: None,
        target_altitude: None,
        altitude: None,
        indicated_airspeed: None,
        true_airspeed: None,
        mach: None,
        power_fraction: None,
        thrust_fraction: None,
        fuel_fraction: None,
        fuel_mass: None,
        payload_mass: None,
        additional_fields: Default::default(),
    }
}

#[test]
fn unadvertised_fields_are_rejected() {
    let mut raw = fixed_fuel();
    raw.fuel_mass = Some("1 kg".to_owned());
    raw.duration = Some("1 min".to_owned());
    let error = resolve_segment(raw, 0).expect_err("duration is not legal for fixed_fuel");
    assert_eq!(error.detail().code, "UNSUPPORTED_SEGMENT_FIELD");
}

#[test]
fn unknown_fields_are_preserved_for_stable_rejection() {
    let raw: RawMissionSegment = serde_yaml::from_str(
        "id: segment\n\
         type: fixed_fuel\n\
         fuel_mass: 1 kg\n\
         typo_field: ignored nowhere\n",
    )
    .expect("raw segment should retain additional fields");
    let error = resolve_segment(raw, 0).expect_err("unknown fields must be rejected");
    assert_eq!(error.detail().code, "UNSUPPORTED_SEGMENT_FIELD");
    assert_eq!(
        error.detail().path.as_deref(),
        Some("mission.segments.0.typo_field")
    );
    assert!(error.detail().message.contains("fixed_fuel"));
}

#[test]
fn exactly_one_fuel_representation_is_required() {
    let missing =
        resolve_segment(fixed_fuel(), 0).expect_err("fixed fuel requires a representation");
    assert_eq!(missing.detail().code, "MISSING_FIXED_FUEL");

    let mut duplicate = fixed_fuel();
    duplicate.fuel_mass = Some("1 kg".to_owned());
    duplicate.fuel_fraction = Some(0.1);
    let error =
        resolve_segment(duplicate, 0).expect_err("fixed fuel representations are exclusive");
    assert_eq!(error.detail().code, "SEGMENT_FIELD_EXCLUSIVITY");

    let mut valid = fixed_fuel();
    valid.fuel_mass = Some("1 kg".to_owned());
    resolve_segment(valid, 0).expect("one representation is valid");
}
