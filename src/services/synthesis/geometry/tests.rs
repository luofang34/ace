use super::*;

#[test]
fn installed_openvsp_wing_adapter_returns_verified_geometry_from_canonical_inputs() {
    let backends = BackendRegistry::detect_blocking();
    let Ok(backend) = backends.openvsp() else {
        tracing::warn!("OpenVSP unavailable; native envelope adapter remains usable");
        return;
    };
    let plugin = Wing::new_blocking(backend.clone()).unwrap();
    let mut inputs = [
        ("area", 100.0, Unit::SquareMeter),
        ("span", 35.0, Unit::Meter),
        ("taper", 0.4, Unit::Ratio),
        ("sweep", 20.0, Unit::Degree),
        ("dihedral", 3.0, Unit::Degree),
        ("twist", 0.0, Unit::Degree),
    ]
    .into_iter()
    .map(|(id, value, unit)| (id.into(), Quantity::new(value, unit).unwrap()))
    .collect();
    let first = plugin
        .evaluate_blocking(&inputs, &ace_engine::RunControl::default())
        .unwrap();
    assert!(first.approximate);
    let area = first.values["wetted_area"].value;
    assert!(area > 190.0 && area < 240.0);
    let artifact = &first.artifacts[0];
    artifact.validate().unwrap();
    assert!(artifact.text.as_deref().unwrap().contains("Vsp_Geometry"));
    inputs.insert("span".into(), Quantity::new(37.0, Unit::Meter).unwrap());
    let second = plugin
        .evaluate_blocking(&inputs, &ace_engine::RunControl::default())
        .unwrap();
    assert_ne!(artifact.digest, second.artifacts[0].digest);
    assert_eq!(
        artifact.generator.implementation,
        plugin.spec.identity.code_revision
    );
}
