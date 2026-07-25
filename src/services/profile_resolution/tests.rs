use std::fs;
use std::path::PathBuf;

use crate::domain::diagnostic::AexError;
use crate::domain::schema::ProfileDocument;

use super::parse_engine_profile;

#[test]
fn b777_table_deck_parses_all_modes_and_axes() -> Result<(), Box<dyn std::error::Error>> {
    let profile = parse_engine_profile(b777_document()?.profile)?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = profile else {
        return Err("B777 requires a turbofan profile".into());
    };
    let table = profile
        .table_deck
        .as_ref()
        .ok_or("B777 requires a table deck")?;
    assert_eq!(table.mach_axis, [0.0, 0.9]);
    assert_eq!(table.altitude_axis_m.len(), 9);
    assert_eq!(table.modes.len(), 4);
    let serialized = serde_json::to_value(&profile)?;
    assert!(serialized.get("table_deck").is_some());
    assert!(serialized.get("sea_level_static_thrust_n").is_none());
    let round_trip: crate::domain::propulsion::TablePropulsionDeck =
        serde_json::from_value(serialized["table_deck"].clone())?;
    assert_eq!(&round_trip, table);
    Ok(())
}

#[test]
fn specialized_table_profiles_preserve_identity_and_fuel_basis()
-> Result<(), Box<dyn std::error::Error>> {
    for (relative, expected_type, expected_id, specific_impulse) in [
        (
            "examples/sr71/profiles/j58.yaml",
            "turbojet_engine",
            "engine.pw_j58_class",
            false,
        ),
        (
            "examples/x15/profiles/xlr99.yaml",
            "rocket_engine",
            "engine.xlr99_class",
            true,
        ),
    ] {
        let document = profile_document(relative)?;
        assert_eq!(document.profile.kind, expected_type);
        let profile = parse_engine_profile(document.profile)?;
        let crate::domain::schema::EngineProfile::Turbofan(profile) = profile else {
            return Err(format!("{expected_type} requires a thrust profile").into());
        };
        assert_eq!(profile.id, expected_id);
        assert_eq!(profile.version, 2);
        assert_eq!(profile.model, "propulsion.table_deck");
        assert!(profile.bypass_ratio.is_none());
        assert!(profile.simple_deck.is_none());
        let deck = profile.table_deck.ok_or("missing table deck")?;
        assert!(deck.modes.iter().all(|mode| {
            matches!(
                mode.fuel,
                crate::domain::propulsion::TableFuelSchedule::SpecificImpulseS(_)
            ) == specific_impulse
        }));
    }
    Ok(())
}

#[test]
fn specialized_thrust_types_require_the_table_model() -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "examples/sr71/profiles/j58.yaml",
        "examples/x15/profiles/xlr99.yaml",
    ] {
        let source = fs::read_to_string(repository_path(relative))?.replace(
            "model: propulsion.table_deck",
            "model: propulsion.turbofan_simple_deck",
        );
        assert_validation_code(&source, "INCOMPATIBLE_PROPULSION_MODEL")?;
    }
    Ok(())
}

#[test]
fn table_deck_rejects_unsorted_axes_bad_shapes_units_and_cells()
-> Result<(), Box<dyn std::error::Error>> {
    let original = b777_source()?;
    let cases = [
        (
            original.replace("mach: [0.0, 0.9]", "mach: [0.9, 0.0]"),
            "UNSORTED_TABLE_AXIS",
        ),
        (
            original.replacen("unit: N", "unit: kN", 1),
            "INCOMPATIBLE_UNITS",
        ),
        (
            original.replacen("              - [513000.000000, 374490.000000]\n", "", 1),
            "INVALID_TABLE_MATRIX_DIMENSIONS",
        ),
        (
            original.replacen("513000.000000", "-1.0", 1),
            "INVALID_TABLE_CELL",
        ),
    ];
    for (source, expected_code) in cases {
        assert_validation_code(&source, expected_code)?;
    }
    Ok(())
}

#[test]
fn table_deck_requires_the_canonical_mode_and_fuel_vocabulary()
-> Result<(), Box<dyn std::error::Error>> {
    let original = b777_source()?;
    let missing_economy = original.replace(
        "        economy:\n          thrust: *thrust_table\n          tsfc: *cruise_tsfc_table\n",
        "",
    );
    let unsupported_mode = original.replace("        economy:", "        boost:");
    let both_fuel_forms = original.replace(
        "        economy:\n          thrust: *thrust_table\n          tsfc: *cruise_tsfc_table\n",
        "        economy:\n          thrust: *thrust_table\n          tsfc: *cruise_tsfc_table\n          specific_impulse: *cruise_tsfc_table\n",
    );
    for (source, expected_code) in [
        (missing_economy, "MISSING_TABLE_MODE"),
        (unsupported_mode, "UNSUPPORTED_PROPULSION_MODE"),
        (both_fuel_forms, "TABLE_FUEL_FIELD_EXCLUSIVITY"),
    ] {
        assert_validation_code(&source, expected_code)?;
    }
    Ok(())
}

#[test]
fn specific_impulse_is_a_valid_alternative_to_tsfc() -> Result<(), Box<dyn std::error::Error>> {
    let source = b777_source()?
        .replace("tsfc:", "specific_impulse:")
        .replace("unit: kg/N/hr", "unit: s")
        .replace("0.0108", "300.0")
        .replace("0.0372", "300.0");
    let document: ProfileDocument = serde_yaml::from_str(&source)?;
    let profile = parse_engine_profile(document.profile)?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = profile else {
        return Err("B777 requires a turbofan profile".into());
    };
    assert!(profile.table_deck.is_some());
    Ok(())
}

#[test]
fn legacy_simple_deck_keeps_its_flat_serialized_shape() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profiles/engines/htf7700l.yaml");
    let document: ProfileDocument = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let profile = parse_engine_profile(document.profile)?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = profile else {
        return Err("HTF7700L requires a turbofan profile".into());
    };
    assert!(profile.simple_deck.is_some());
    assert!(profile.table_deck.is_none());
    let serialized = serde_json::to_value(profile)?;
    assert!(serialized.get("simple_deck").is_none());
    assert!(serialized.get("sea_level_static_thrust_n").is_some());
    Ok(())
}

#[test]
fn table_and_simple_deck_forms_are_mutually_exclusive() -> Result<(), Box<dyn std::error::Error>> {
    let table_with_simple = b777_source()?.replace(
        "    dry_mass: 8760 kg\n",
        "    sea_level_static_thrust: 513 kN\n    dry_mass: 8760 kg\n",
    );
    assert_validation_code(&table_with_simple, "PROPULSION_DECK_EXCLUSIVITY")?;

    let wrong_model = b777_source()?.replace(
        "model: propulsion.table_deck",
        "model: propulsion.turbofan_simple_deck",
    );
    assert_validation_code(&wrong_model, "INCOMPATIBLE_PROPULSION_MODEL")?;
    Ok(())
}

#[test]
fn sea_level_reference_thrust_is_interpolated_from_declared_axes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = b777_document()?;
    document.profile.parameters["table_deck"]["axes"]["altitude"] =
        serde_yaml::to_value(["-1000 m", "1000 m"])?;
    for mode in ["takeoff", "climb", "cruise", "economy"] {
        let data = &mut document.profile.parameters["table_deck"]["modes"][mode];
        data["thrust"]["values"] = serde_yaml::to_value([[100.0, 100.0], [300.0, 300.0]])?;
        data["tsfc"]["values"] = serde_yaml::to_value([[0.01, 0.01], [0.01, 0.01]])?;
    }
    let profile = parse_engine_profile(document.profile)?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = profile else {
        return Err("B777 requires a turbofan profile".into());
    };
    assert!((profile.installed_reference_thrust_n(1, 1.0) - 200.0 * 0.97).abs() < 1.0e-12);
    Ok(())
}

fn assert_validation_code(
    source: &str,
    expected_code: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    let document: ProfileDocument = serde_yaml::from_str(source)?;
    let error = match parse_engine_profile(document.profile) {
        Err(error) => error,
        Ok(_) => return Err(format!("expected validation error for {expected_code}").into()),
    };
    let AexError::Validation { code, .. } = error else {
        return Err(format!("expected validation error for {expected_code}").into());
    };
    assert_eq!(code, expected_code);
    Ok(())
}

fn b777_document() -> Result<ProfileDocument, Box<dyn std::error::Error>> {
    Ok(serde_yaml::from_str(&b777_source()?)?)
}

fn profile_document(relative: &str) -> Result<ProfileDocument, Box<dyn std::error::Error>> {
    Ok(serde_yaml::from_str(&fs::read_to_string(
        repository_path(relative),
    )?)?)
}

fn b777_source() -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(repository_path(
        "examples/b777/profiles/engine.yaml",
    ))?)
}

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}
