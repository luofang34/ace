use super::*;
use ace_engine::{Session, Snapshot};
use ace_model::Source;

fn number(snapshot: &Snapshot, id: &str) -> f64 {
    snapshot.values[id].quantity.unwrap().value
}

fn edit(session: &mut Session, id: &str, value: f64, unit: Unit) {
    session
        .edit(
            &BTreeMap::from([(id.into(), Quantity::new(value, unit).unwrap())]),
            Source {
                kind: "test".into(),
                reference: "VSPAERO dependency and closure verification".into(),
                revision: None,
            },
        )
        .unwrap();
}

#[test]
fn installed_vspaero_drives_fuel_closure_and_preserves_hard_constraints() {
    let backends = BackendRegistry::detect_blocking();
    let Ok(backend) = backends.openvsp() else {
        tracing::warn!("OpenVSP unavailable: installed VSPAERO integration test skipped");
        return;
    };
    let plugin = PolarPlugin::new_blocking(backend.clone()).unwrap();
    assert!(plugin.binary.contains("vspaero:sha256:"));
    let model = serde_json::from_str(include_str!(
        "../../../../examples/synthesis/vspaero-transport.json"
    ))
    .unwrap();
    let mut session = super::super::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let baseline = session.run().unwrap();
    assert!(
        baseline.complete(),
        "{:?}",
        baseline
            .values
            .iter()
            .filter(|(_, v)| !v.usable())
            .collect::<Vec<_>>()
    );
    let polar_id = baseline.values["aero.vspaero.k_020"].run_id.clone();
    verify_polar(&baseline);
    assert!(baseline.closures["mass_fuel"].converged);
    let cached = session.run().unwrap();
    assert!(cached.executed.is_empty());
    edit(&mut session, "payload.mass", 19000.0, Unit::Kilogram);
    let heavy = session.run().unwrap();
    assert!(heavy.complete());
    assert_eq!(heavy.values["aero.vspaero.k_020"].run_id, polar_id);
    assert!(number(&heavy, "fuel.required") > number(&baseline, "fuel.required"));
    edit(&mut session, "wing.span", 37.0, Unit::Meter);
    assert!(!session.snapshot().values["economics.casm"].usable());
    let wider = session.run().unwrap();
    assert!(wider.complete());
    assert_ne!(wider.values["aero.vspaero.k_020"].run_id, polar_id);
    assert!(number(&wider, "aero.vspaero.k_020") < number(&heavy, "aero.vspaero.k_020"));
    assert!(number(&wider, "fuel.required") < number(&heavy, "fuel.required"));
    assert!(number(&wider, "economics.casm") < number(&heavy, "economics.casm"));
    let sheet = ace_sheet::project(session.model(), &wider).unwrap();
    assert_eq!(sheet.snapshot.revision, session.model().revision);
    assert_eq!(
        sheet.assessment.feasibility,
        ace_engine::Feasibility::Violated
    );
    assert!(sheet.assessment.violations.contains(&"gate_span".into()));
    verify_reopen_and_capacity(&mut session, &wider);
}

fn verify_polar(snapshot: &Snapshot) {
    let id = snapshot.values["aero.vspaero.k_020"]
        .run_id
        .as_ref()
        .unwrap();
    let run = snapshot.runs.iter().find(|run| &run.id == id).unwrap();
    assert_eq!(run.artifacts.len(), 2);
    for artifact in &run.artifacts {
        artifact.validate().unwrap();
    }
    let evidence: serde_json::Value =
        serde_json::from_str(run.artifacts[1].text.as_deref().unwrap()).unwrap();
    assert_eq!(evidence["points"].as_array().unwrap().len(), 18);
    for field in ["RootThickness", "TipThickness"] {
        assert_eq!(evidence["readback"][field], 0.12);
    }
    for field in ["RootCamber", "TipCamber"] {
        assert_eq!(evidence["readback"][field], 0.0);
    }
    let k = number(snapshot, "aero.vspaero.k_020");
    // Finite-wing induced drag should be of lifting-line order, not zero or parasite drag.
    let ideal = 1.0 / (std::f64::consts::PI * 35.0_f64.powi(2) / 140.0);
    assert!((0.7 * ideal..1.5 * ideal).contains(&k));
    assert!(
        run.details["fits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|fit| fit["maximum_relative_error"].as_f64().unwrap() < 0.06)
    );
}

fn verify_reopen_and_capacity(session: &mut Session, wider: &Snapshot) {
    let encoded = serde_json::to_string(session.model()).unwrap();
    let model = serde_json::from_str(&encoded).unwrap();
    let mut reopened = super::super::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let replay = reopened.run().unwrap();
    for id in ["aero.vspaero.k_020", "fuel.required", "economics.casm"] {
        assert!((number(&replay, id) - number(wider, id)).abs() < 1e-8);
    }
    edit(session, "mission.distance", 5000.0, Unit::NauticalMile);
    let insufficient = session.run().unwrap();
    assert!(!insufficient.complete());
    assert!(!insufficient.values["fuel.required"].usable());
    assert!(!insufficient.values["economics.casm"].usable());
    assert_eq!(
        insufficient.values["aero.vspaero.k_020"].run_id,
        wider.values["aero.vspaero.k_020"].run_id
    );
}

#[test]
fn unsupported_airfoil_or_twist_is_rejected_before_launch() {
    let inputs = [
        ("area", 140.),
        ("span", 35.),
        ("taper", 0.4),
        ("sweep", 25.),
        ("dihedral", 5.),
        ("twist", 0.),
        ("thickness", 0.12),
    ];
    let mut values = inputs
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
    assert!(validate(&values).is_ok());
    values.insert("twist".into(), -2.0);
    assert!(validate(&values).is_err());
    values.insert("twist".into(), 0.0);
    values.insert("thickness".into(), 0.15);
    assert!(validate(&values).is_err());
}

#[test]
fn missing_polar_solver_leaves_mission_and_economics_unavailable() {
    let mut model: VehicleModel = serde_json::from_str(include_str!(
        "../../../../examples/synthesis/vspaero-transport.json"
    ))
    .unwrap();
    model.analyses.get_mut("vspaero_polar").unwrap().version = "uninstalled".into();
    let mut session = super::super::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    for id in ["aero.vspaero.k_020", "fuel.required", "economics.casm"] {
        assert!(!snapshot.values[id].usable());
    }
    assert_eq!(
        ace_engine::assess(session.model(), &snapshot)
            .unwrap()
            .feasibility,
        ace_engine::Feasibility::Unknown
    );
}
