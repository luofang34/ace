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
    verify_planform(&first, 35.0);
    assert!(first.approximate);
    let area = first.values["wetted_area"].value;
    assert!(area > 190.0 && area < 240.0);
    let artifact = &first.artifacts[0];
    artifact.validate().unwrap();
    assert!(artifact.text.as_deref().unwrap().contains("Vsp_Geometry"));
    let repeated = plugin
        .evaluate_blocking(&inputs, &ace_engine::RunControl::default())
        .unwrap();
    assert_eq!(
        repeated.artifacts[0].digest, artifact.digest,
        "identical canonical inputs must reproduce the kernel artifact byte for byte"
    );
    assert!(artifact.text.as_deref().unwrap().contains("ID=\"AAAAAAAA"));
    inputs.insert("span".into(), Quantity::new(37.0, Unit::Meter).unwrap());
    let second = plugin
        .evaluate_blocking(&inputs, &ace_engine::RunControl::default())
        .unwrap();
    verify_planform(&second, 37.0);
    assert_ne!(artifact.digest, second.artifacts[0].digest);
    assert_eq!(
        artifact.generator.implementation,
        plugin.spec.identity.code_revision
    );
}

fn xml_value(text: &str, field: &str) -> f64 {
    text.split_once(&format!("<{field} Value=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn verify_planform(output: &PluginOutput, span: f64) {
    let text = output.artifacts[0].text.as_deref().unwrap();
    let projected = xml_value(text, "TotalProjectedSpan");
    let total_area = xml_value(text, "TotalArea");
    let area = total_area * 3.0_f64.to_radians().cos();
    assert!((projected - span).abs() < 1e-9);
    assert!((area - 100.0).abs() < 1e-9);
    // The trapezoid reference area is independently reconstructed from measured section chords.
    let section = text.rsplit_once("<Root_Chord Value=\"").unwrap().1;
    let root: f64 = section.split('"').next().unwrap().parse().unwrap();
    let tip = xml_value(section, "Tip_Chord");
    assert!((0.5 * (root + tip) * projected - 100.0).abs() < 1e-9);
    assert!((tip / root - 0.4).abs() < 1e-12);
}
