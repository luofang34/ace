#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::domain::aerodynamics::PolarTable;
use crate::domain::diagnostic::AexError;
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RawMissionInitialState,
    RequirementsDocument, ScenarioDocument, WaveDrag,
};
use crate::domain::study::EmbeddedStudyBaseline;
use crate::services::analysis::ApplicationService;
use crate::storage::profile_store::FileProfileStore;

use super::{
    ScenarioResolver, resolve_aircraft, resolve_embedded_study, resolve_mission,
    validate_initial_state,
};

fn valid_polar_table() -> PolarTable {
    PolarTable {
        mach: vec![0.0, 1.0],
        cd0: vec![0.02, 0.03],
        cl_max: vec![1.4, 1.0],
        oswald_efficiency: Some(vec![0.8, 0.6]),
        induced_drag_factor: None,
    }
}

fn c172_aircraft_document() -> Result<AircraftDocument, Box<dyn std::error::Error>> {
    read_c172_document("aircraft.yaml")
}

fn read_c172_document<T: DeserializeOwned>(
    relative: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/c172")
        .join(relative);
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

fn embedded_c172() -> Result<EmbeddedStudyBaseline, Box<dyn std::error::Error>> {
    Ok(EmbeddedStudyBaseline {
        scenario: read_c172_document::<ScenarioDocument>("scenario.yaml")?,
        aircraft: c172_aircraft_document()?,
        mission: read_c172_document::<MissionDocument>("mission.yaml")?,
        requirements: read_c172_document::<RequirementsDocument>("requirements.yaml")?,
        profiles: vec![
            read_c172_document::<ProfileDocument>("profiles/engine.yaml")?,
            read_c172_document::<ProfileDocument>("profiles/propeller.yaml")?,
        ],
    })
}

fn c172_mission_with_initial_state(
    state: RawMissionInitialState,
) -> Result<MissionDocument, Box<dyn std::error::Error>> {
    let mut document = read_c172_document::<MissionDocument>("mission.yaml")?;
    document.mission.initial_state = Some(state);
    Ok(document)
}

fn assert_closed_planform(scenario: &crate::domain::schema::ResolvedScenario) {
    let wing = &scenario.aircraft.wing;
    assert!((wing.span_m.powi(2) / wing.area_m2 - wing.aspect_ratio).abs() < 1.0e-12);
}

fn collect_scenario_paths(
    directory: &std::path::Path,
    paths: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_scenario_paths(&entry.path(), paths)?;
        } else if entry.file_name() == "scenario.yaml" {
            paths.push(entry.path());
        }
    }
    Ok(())
}

#[test]
fn explicit_reference_topology_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let aircraft = resolve_aircraft(c172_aircraft_document()?)?;

    assert!(!aircraft.topology.inferred);
    assert!(aircraft.topology.has_component_kind("horizontal_tail"));
    assert_eq!(aircraft.topology.relationships.len(), 6);
    let fuselage = aircraft
        .geometry
        .fuselage
        .ok_or_else(|| std::io::Error::other("missing resolved fuselage"))?;
    assert_eq!(fuselage.length.value, 8.25);
    assert!(fuselage.length.provenance.explicitly_provided);
    assert_eq!(fuselage.length.provenance.kind, "user_input");
    Ok(())
}

#[test]
fn absent_topology_preserves_legacy_inference() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document.aircraft.topology = None;
    let aircraft = resolve_aircraft(document)?;

    assert!(aircraft.topology.inferred);
    assert!(aircraft.topology.has_component_kind("fuselage"));
    assert!(aircraft.topology.has_component_kind("propeller"));
    assert!(!aircraft.topology.has_component_kind("lifting_body"));
    Ok(())
}

#[test]
fn aircraft_resolves_additive_polar_table() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document.aircraft.aerodynamics.clean.polar_table = Some(valid_polar_table());
    let aircraft = resolve_aircraft(document)?;

    let table = aircraft
        .aerodynamics
        .clean
        .polar_table
        .ok_or_else(|| std::io::Error::other("polar table was not resolved"))?;
    assert_eq!(table.mach, vec![0.0, 1.0]);
    Ok(())
}

#[test]
fn polar_table_validation_rejects_malformed_data() {
    let mut short_axis = valid_polar_table();
    short_axis.mach = vec![0.0];
    let mut unsorted_axis = valid_polar_table();
    unsorted_axis.mach = vec![1.0, 0.0];
    let mut negative_axis = valid_polar_table();
    negative_axis.mach = vec![-0.1, 1.0];
    let mut bad_dimensions = valid_polar_table();
    bad_dimensions.cd0 = vec![0.02];
    let mut bad_cell = valid_polar_table();
    bad_cell.cl_max[1] = 0.0;
    let mut nonfinite_cell = valid_polar_table();
    nonfinite_cell.cd0[1] = f64::NAN;
    let mut bad_efficiency = valid_polar_table();
    bad_efficiency.oswald_efficiency = Some(vec![0.8, 1.1]);
    let mut missing_induced = valid_polar_table();
    missing_induced.oswald_efficiency = None;
    let mut conflicting_induced = valid_polar_table();
    conflicting_induced.induced_drag_factor = Some(vec![0.1, 0.2]);

    for (table, expected) in [
        (short_axis, "INVALID_POLAR_TABLE_AXIS"),
        (unsorted_axis, "UNSORTED_POLAR_TABLE_AXIS"),
        (negative_axis, "INVALID_POLAR_TABLE_AXIS"),
        (bad_dimensions, "INVALID_POLAR_TABLE_DIMENSIONS"),
        (bad_cell, "INVALID_POLAR_TABLE_CELL"),
        (nonfinite_cell, "INVALID_POLAR_TABLE_CELL"),
        (bad_efficiency, "INVALID_OSWALD_EFFICIENCY"),
        (missing_induced, "POLAR_INDUCED_FIELD_EXCLUSIVITY"),
        (conflicting_induced, "POLAR_INDUCED_FIELD_EXCLUSIVITY"),
    ] {
        let error = super::polar::resolve(Some(table), None, "clean")
            .expect_err("malformed polar table must fail");
        assert_eq!(error.detail().code, expected);
        assert!(
            error
                .detail()
                .path
                .as_deref()
                .is_some_and(|path| path.starts_with("aircraft.aerodynamics.clean.polar_table"))
        );
    }
}

#[test]
fn polar_table_rejects_legacy_wave_drag() {
    let error = super::polar::resolve(
        Some(valid_polar_table()),
        Some(&WaveDrag {
            coefficient: 0.01,
            exponent: 0.5,
        }),
        "clean",
    )
    .expect_err("overlapping Mach drag definitions must fail");

    assert_eq!(error.detail().code, "POLAR_MODEL_CONFLICT");
    assert_eq!(
        error.detail().path.as_deref(),
        Some("aircraft.aerodynamics.clean.polar_table")
    );
}

#[test]
fn embedded_study_applies_overrides_before_cross_document_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    embedded
        .scenario
        .scenario
        .overrides
        .insert("mission.payload.mass".to_owned(), "400 kg".to_owned());

    assert!(matches!(
        resolve_embedded_study(&embedded),
        Err(AexError::Validation {
            code: "PAYLOAD_LIMIT_EXCEEDED",
            ..
        })
    ));
    Ok(())
}

#[test]
fn embedded_study_requires_referenced_profiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    embedded.aircraft.aircraft.propulsion.profile = "engine.missing".to_owned();

    assert!(matches!(
        resolve_embedded_study(&embedded),
        Err(AexError::ProfileNotFound { profile_id, .. }) if profile_id == "engine.missing"
    ));
    Ok(())
}

#[test]
fn embedded_study_retains_profile_sanity_warnings() -> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    let engine = embedded
        .profiles
        .iter_mut()
        .find(|profile| profile.profile.kind == "piston_engine")
        .ok_or_else(|| std::io::Error::other("missing embedded piston profile"))?;
    let parameters = engine
        .profile
        .parameters
        .as_mapping_mut()
        .ok_or_else(|| std::io::Error::other("profile parameters are not a mapping"))?;
    parameters.insert(
        serde_yaml::Value::String("rated_altitude".to_owned()),
        serde_yaml::Value::String("-1000 m".to_owned()),
    );

    let scenario = resolve_embedded_study(&embedded)?;

    assert!(scenario.warnings.iter().any(|warning| {
        warning.code == "PARAMETER_OUTSIDE_TYPICAL"
            && warning.path.as_deref() == Some("profile.parameters.rated_altitude")
    }));
    Ok(())
}

#[test]
fn inconsistent_override_is_rejected_and_paired_override_closes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");

    assert!(matches!(
        service.resolve_blocking(
            &scenario,
            &BTreeMap::from([(
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "20".to_owned()
            )])
        ),
        Err(AexError::Validation {
            code: "INCONSISTENT_WING_PLANFORM",
            ..
        })
    ));
    let paired = service.resolve_blocking(
        &scenario,
        &BTreeMap::from([
            (
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "8".to_owned(),
            ),
            (
                "aircraft.geometry.wing.span".to_owned(),
                format!("{} m", (16.17_f64 * 8.0).sqrt()),
            ),
        ]),
    )?;
    assert_closed_planform(&paired);
    Ok(())
}

#[test]
fn template_engine_count_must_match_resolved_aircraft() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/b777/scenario.yaml");
    let result = service.resolve_blocking(
        &scenario,
        &BTreeMap::from([(
            "requirements.template.engine_count".to_owned(),
            "3".to_owned(),
        )]),
    );

    assert!(matches!(
        result,
        Err(AexError::Validation {
            code: "TEMPLATE_ENGINE_COUNT_MISMATCH",
            path,
            ..
        }) if path == "requirements.template.engine_count"
    ));
    Ok(())
}

#[test]
fn every_shipped_example_resolves_a_closed_planform() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = ScenarioResolver::new(Arc::new(FileProfileStore));
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths = Vec::new();
    collect_scenario_paths(&examples, &mut paths)?;
    paths.sort();
    assert!(!paths.is_empty());

    for path in paths {
        let scenario = resolver.resolve_blocking(&path, &BTreeMap::new())?;
        assert_closed_planform(&scenario);
    }
    Ok(())
}

#[test]
fn mission_initial_state_enforces_representation_exclusivity()
-> Result<(), Box<dyn std::error::Error>> {
    for state in [
        RawMissionInitialState {
            true_airspeed: Some("100 kt".to_owned()),
            mach: Some(0.2),
            ..RawMissionInitialState::default()
        },
        RawMissionInitialState {
            fuel_fraction: Some(0.5),
            fuel_mass: Some("50 kg".to_owned()),
            ..RawMissionInitialState::default()
        },
    ] {
        assert!(matches!(
            resolve_mission(c172_mission_with_initial_state(state)?),
            Err(AexError::Validation {
                code: "INITIAL_STATE_FIELD_EXCLUSIVITY",
                ..
            })
        ));
    }
    Ok(())
}

#[test]
fn mission_initial_state_rejects_unknown_and_invalid_values()
-> Result<(), Box<dyn std::error::Error>> {
    let mut unknown = RawMissionInitialState::default();
    unknown
        .additional_fields
        .insert("engine_off".to_owned(), serde_yaml::Value::Bool(true));
    let cases = [
        (
            unknown,
            "UNSUPPORTED_INITIAL_STATE_FIELD",
            "mission.initial_state.engine_off",
        ),
        (
            RawMissionInitialState {
                fuel_fraction: Some(1.01),
                ..RawMissionInitialState::default()
            },
            "INVALID_INITIAL_FUEL_FRACTION",
            "mission.initial_state.fuel_fraction",
        ),
        (
            RawMissionInitialState {
                altitude: Some("-1 ft".to_owned()),
                ..RawMissionInitialState::default()
            },
            "NEGATIVE_VALUE",
            "mission.initial_state.altitude",
        ),
    ];
    for (state, expected_code, expected_path) in cases {
        let error = resolve_mission(c172_mission_with_initial_state(state)?)
            .err()
            .ok_or_else(|| std::io::Error::other("invalid initial state resolved"))?;
        assert_eq!(error.detail().code, expected_code);
        assert_eq!(error.detail().path.as_deref(), Some(expected_path));
    }
    Ok(())
}

#[test]
fn mission_initial_state_accepts_empty_and_zero_fuel_starts()
-> Result<(), Box<dyn std::error::Error>> {
    let legacy = resolve_mission(read_c172_document::<MissionDocument>("mission.yaml")?)?;
    assert!(legacy.initial_state.is_none());

    let explicit = resolve_mission(c172_mission_with_initial_state(RawMissionInitialState {
        altitude: Some("5000 ft".to_owned()),
        indicated_airspeed: Some("90 kt".to_owned()),
        fuel_fraction: Some(0.0),
        ..RawMissionInitialState::default()
    })?)?;
    let state = explicit
        .initial_state
        .ok_or_else(|| std::io::Error::other("initial state was not resolved"))?;
    assert!((state.altitude_m.unwrap_or_default() - 1_524.0).abs() < 1.0e-8);
    assert_eq!(state.fuel_fraction, Some(0.0));
    Ok(())
}

#[test]
fn initial_state_is_checked_against_aircraft_loading_and_limits()
-> Result<(), Box<dyn std::error::Error>> {
    let mut aircraft = resolve_aircraft(c172_aircraft_document()?)?;
    aircraft.limits.maximum_operating_altitude_m = Some(3_000.0);
    aircraft.limits.maximum_operating_mach = Some(0.5);
    for (state, expected_code) in [
        (
            RawMissionInitialState {
                fuel_mass: Some("500 kg".to_owned()),
                ..RawMissionInitialState::default()
            },
            "INITIAL_FUEL_EXCEEDS_USABLE",
        ),
        (
            RawMissionInitialState {
                altitude: Some("50000 ft".to_owned()),
                ..RawMissionInitialState::default()
            },
            "INITIAL_ALTITUDE_LIMIT_EXCEEDED",
        ),
        (
            RawMissionInitialState {
                mach: Some(1.0),
                ..RawMissionInitialState::default()
            },
            "INITIAL_MACH_LIMIT_EXCEEDED",
        ),
    ] {
        let mission = resolve_mission(c172_mission_with_initial_state(state)?)?;
        let error = validate_initial_state(&aircraft, &mission)
            .err()
            .ok_or_else(|| std::io::Error::other("invalid initial state passed limits"))?;
        assert_eq!(error.detail().code, expected_code);
    }
    Ok(())
}
