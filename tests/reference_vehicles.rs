//! Real-aircraft reference vehicles keep plausible, legacy-consistent results.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn synthesize(name: &str) -> Result<Value, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("vehicle.json");
    let output = cargo_bin_cmd!("aex")
        .arg("synthesize")
        .arg(path)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn value(result: &Value, parameter: &str) -> Result<f64, Box<dyn Error>> {
    result["snapshot"]["values"][parameter]["quantity"]["value"]
        .as_f64()
        .ok_or_else(|| format!("{parameter} is unavailable").into())
}

#[test]
fn c172_vehicle_flies_its_mission_with_reference_performance() -> Result<(), Box<dyn Error>> {
    let result = synthesize("c172")?;
    assert_eq!(result["completed"], true);
    assert_eq!(result["assessment"]["feasibility"], "provisional");
    assert!((value(&result, "mission.flown_distance")? - 350.0 * 1852.0).abs() < 1.0);
    // The scenario mission burns 86 kg including reserve with power-fraction schedules;
    // constant-efficiency vehicle phases must stay within that class of result.
    let fuel = value(&result, "mission.block_fuel")? + value(&result, "mission.reserve_fuel")?;
    assert!((60.0..=100.0).contains(&fuel), "{fuel}");
    assert!(value(&result, "mission.remaining_fuel")? > 0.0);
    let stall_kt = value(&result, "performance.stall_speed_landing")? / (1852.0 / 3600.0);
    assert!((40.0..=50.0).contains(&stall_kt), "{stall_kt}");
    let ceiling_ft = value(&result, "performance.service_ceiling")? / 0.3048;
    assert!((11_000.0..=14_000.0).contains(&ceiling_ft), "{ceiling_ft}");
    assert_eq!(result["margins"]["stall_speed_landing"]["status"], "PASS");
    assert_eq!(result["margins"]["service_ceiling"]["status"], "FAIL");
    assert_constraint_diagram_met(&result, 600.0..=700.0)?;
    assert_trimmed_cruise_held(&result, 0.5..=3.0)?;
    Ok(())
}

/// MTOM wing loading in the expected band, inside the stall limit and under installed loading.
fn assert_constraint_diagram_met(
    result: &Value,
    wing_loading_pa: std::ops::RangeInclusive<f64>,
) -> Result<(), Box<dyn Error>> {
    let selected = value(result, "constraints.selected_wing_loading")?;
    assert!(wing_loading_pa.contains(&selected), "{selected}");
    for (parameter, requirement) in [
        (
            "constraints.installed_to_required_loading",
            "installed_loading",
        ),
        (
            "constraints.stall_limit_to_selected_wing_loading",
            "landing_wing_loading",
        ),
    ] {
        let ratio = value(result, parameter)?;
        assert!((1.0..=2.0).contains(&ratio), "{parameter} {ratio}");
        assert_ne!(result["margins"][requirement]["status"], "FAIL");
    }
    Ok(())
}

/// The CG-trimmed cruise point at takeoff mass climbs, with the tail carrying a small download.
fn assert_trimmed_cruise_held(
    result: &Value,
    rate_of_climb: std::ops::RangeInclusive<f64>,
) -> Result<(), Box<dyn Error>> {
    let climb = value(result, "cruise_point.rate_of_climb")?;
    assert!(rate_of_climb.contains(&climb), "{climb}");
    let fraction = value(result, "cruise_point.trim_wing_lift_fraction")?;
    assert!((1.0..=1.05).contains(&fraction), "{fraction}");
    assert_eq!(result["margins"]["cruise_excess_power"]["status"], "PASS");
    Ok(())
}

fn study(name: &str) -> Result<ace_study::StudyArchive, Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name);
    let output = cargo_bin_cmd!("aex")
        .arg("study")
        .arg(root.join("vehicle.json"))
        .arg("--plan")
        .arg(root.join("vehicle-study.json"))
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let archive: ace_study::StudyArchive = serde_json::from_slice(&output.stdout)?;
    assert_eq!(archive.candidates.len(), 81);
    assert_eq!(archive.stop_reason, ace_study::StopReason::GridExhausted);
    assert!(!archive.provisional_pareto.is_empty());
    Ok(archive)
}

#[test]
fn c172_vehicle_study_explores_the_reference_trade_grid() -> Result<(), Box<dyn Error>> {
    let archive = study("c172")?;
    assert!(
        archive
            .candidates
            .iter()
            .all(|candidate| candidate.feasibility != ace_study::Feasibility::Unknown)
    );
    Ok(())
}

#[test]
fn b777_vehicle_flies_its_mission_on_the_tabulated_engine_deck() -> Result<(), Box<dyn Error>> {
    let result = synthesize("b777")?;
    assert_eq!(result["completed"], true);
    assert_eq!(result["assessment"]["feasibility"], "provisional");
    assert!((value(&result, "mission.flown_distance")? - 7_300.0 * 1852.0).abs() < 1.0);
    // The scenario mission burns 106.5 t including alternate and hold, leaving 25.2 t.
    let fuel = value(&result, "mission.block_fuel")? + value(&result, "mission.reserve_fuel")?;
    assert!((96_000.0..=117_000.0).contains(&fuel), "{fuel}");
    let remaining = value(&result, "mission.remaining_fuel")?;
    assert!((15_000.0..=35_000.0).contains(&remaining), "{remaining}");
    let ceiling_ft = value(&result, "performance.service_ceiling")? / 0.3048;
    assert!((39_000.0..=45_000.0).contains(&ceiling_ft), "{ceiling_ft}");
    let lift_to_drag = value(&result, "performance.maximum_lift_to_drag")?;
    assert!((16.0..=22.0).contains(&lift_to_drag), "{lift_to_drag}");
    for requirement in ["service_ceiling", "takeoff_field_length"] {
        assert_eq!(result["margins"][requirement]["status"], "PASS");
    }
    assert_constraint_diagram_met(&result, 7_500.0..=8_200.0)?;
    assert_trimmed_cruise_held(&result, 2.0..=15.0)?;
    let runs = result["snapshot"]["runs"].to_string();
    assert!(runs.contains("aircraft.deck_energy_path_phase"));
    Ok(())
}

#[test]
fn b777_vehicle_study_leaves_only_underpowered_climbs_unresolved() -> Result<(), Box<dyn Error>> {
    let archive = study("b777")?;
    let unknown = archive
        .candidates
        .iter()
        .filter(|candidate| candidate.feasibility == ace_study::Feasibility::Unknown)
        .collect::<Vec<_>>();
    // Undersized engines on the largest wing cannot hold the fixed climb schedule;
    // the mission fails explicitly rather than reporting a guessed verdict.
    assert!(
        !unknown.is_empty() && unknown.len() < 10,
        "{}",
        unknown.len()
    );
    for candidate in unknown {
        assert_eq!(candidate.updates["engine.sizing_factor"].value, 0.92);
    }
    Ok(())
}

/// Range in nautical miles at each loading corner, in design, payload, fuel, ferry order.
fn payload_range(name: &str) -> Result<Vec<(f64, Value)>, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("vehicle.json");
    let output = cargo_bin_cmd!("aex")
        .arg("payload-range")
        .arg(path)
        .args(["--distance", "mission.distance"])
        .args(["--remaining", "mission.remaining_fuel"])
        .args(["--maximum-takeoff-mass", "weight.mtow"])
        .args(["--maximum-payload", "weight.maximum_payload"])
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let points = result["points"].as_array().ok_or("missing points")?;
    let ids: Vec<_> = points.iter().map(|point| point["id"].clone()).collect();
    assert_eq!(ids, ["design", "maximum_payload", "maximum_fuel", "ferry"]);
    points
        .iter()
        .map(|point| {
            assert_eq!(point["range"]["unit"], "nmi");
            let range = point["range"]["value"].as_f64().ok_or("missing range")?;
            Ok((range, point.clone()))
        })
        .collect()
}

#[test]
fn c172_payload_range_flies_its_loading_corners() -> Result<(), Box<dyn Error>> {
    let points = payload_range("c172")?;
    let [design, payload, fuel, ferry] = [0, 1, 2, 3].map(|index| points[index].0);
    // MTOM leaves no fuel at maximum payload, so that corner cannot fly.
    assert_eq!(payload, 0.0);
    assert!(points[1].1["note"].as_str().is_some());
    // The design mission completes at 350 nmi with fuel to spare.
    assert!(
        design > 350.0 && fuel > design && ferry > fuel,
        "{design} {fuel} {ferry}"
    );
    assert!(ferry < 1_000.0, "{ferry}");
    Ok(())
}

#[test]
fn b777_payload_range_brackets_the_scenario_corners() -> Result<(), Box<dyn Error>> {
    let points = payload_range("b777")?;
    // Representative-cruise scenario corners for the same loadings, nmi.
    let scenario = [9_203.0, 7_905.0, 10_315.0, 11_206.0];
    for ((range, point), expected) in points.iter().zip(scenario) {
        assert!(
            (range / expected - 1.0).abs() < 0.1,
            "{range} vs {expected}"
        );
        let width = point["bracket_width"]["value"]
            .as_f64()
            .ok_or("missing width")?;
        assert!(width <= 1e-3 * range, "{width}");
    }
    let [design, payload, fuel, ferry] = [0, 1, 2, 3].map(|index| points[index].0);
    assert!(payload < design && design < fuel && fuel < ferry);
    assert!(design > 7_300.0, "{design}");
    Ok(())
}

#[test]
fn vehicle_constraint_and_payload_range_charts_render_from_run_evidence()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/b777");
    let diagram = directory.path().join("constraints.svg");
    let output = cargo_bin_cmd!("aex")
        .arg("chart")
        .arg(root.join("vehicle.json"))
        .arg("constraints")
        .arg("--artifact")
        .arg(&diagram)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let chart: Value = serde_json::from_slice(&output.stdout)?;
    let curves: Vec<_> = chart["spec"]["series"]
        .as_array()
        .ok_or("missing series")?
        .iter()
        .map(|series| series["id"].clone())
        .collect();
    assert_eq!(
        curves,
        ["climb_rate", "cruise_speed", "takeoff_distance_approximate"]
    );
    let selected = chart["spec"]["annotations"][0]["x"]
        .as_f64()
        .ok_or("no annotation")?;
    assert!((7_500.0..=8_200.0).contains(&selected), "{selected}");
    assert!(std::fs::read_to_string(&diagram)?.starts_with("<svg"));

    let corners = directory.path().join("payload-range.svg");
    let output = cargo_bin_cmd!("aex")
        .arg("payload-range")
        .arg(root.join("vehicle.json"))
        .args(["--distance", "mission.distance"])
        .args(["--remaining", "mission.remaining_fuel"])
        .args(["--maximum-takeoff-mass", "weight.mtow"])
        .args(["--maximum-payload", "weight.maximum_payload"])
        .arg("--artifact")
        .arg(&corners)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["points"].as_array().map(Vec::len), Some(4));
    assert_eq!(result["chart"]["spec"]["x"]["unit"], "nmi");
    assert_eq!(
        result["chart"]["spec"]["annotations"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    assert!(std::fs::read_to_string(&corners)?.starts_with("<svg"));
    Ok(())
}

#[test]
fn out_of_domain_fixtures_fail_explicitly_through_validity_domains() -> Result<(), Box<dyn Error>> {
    for name in ["sr71", "x15"] {
        let result = synthesize(name)?;
        assert_eq!(result["completed"], false, "{name}");
        assert_eq!(result["assessment"]["feasibility"], "unknown", "{name}");
        let values = &result["snapshot"]["values"];
        // The declared cruise or boost altitude lies above the flight phases' domain.
        let mission = &values["mission.block_fuel"];
        assert_eq!(mission["status"], "failed", "{name}");
        let message = mission["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("outside solver domain") && message.contains("target_altitude"),
            "{name}: {message}"
        );
        // The airframe itself lies outside the registered conceptual model domains.
        let ceiling = &values["performance.service_ceiling"];
        assert_eq!(ceiling["status"], "failed", "{name}");
        let message = ceiling["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("MODEL_DOMAIN_UNSUPPORTED"),
            "{name}: {message}"
        );
        for (requirement, margin) in result["margins"].as_object().ok_or("margins")? {
            let status = margin["status"].as_str().unwrap_or_default();
            assert!(
                requirement.contains("payload") || status == "UNKNOWN",
                "{name} {requirement} reported {status} without evidence"
            );
        }
    }
    Ok(())
}
