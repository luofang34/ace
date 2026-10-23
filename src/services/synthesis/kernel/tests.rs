use super::*;
use ace_engine::RunControl;
use ace_physics::Frame;

fn rotation([x, y, z]: [f64; 3]) -> [[f64; 3]; 3] {
    let (sx, cx) = x.to_radians().sin_cos();
    let (sy, cy) = y.to_radians().sin_cos();
    let (sz, cz) = z.to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    multiply(rx, multiply(ry, rz))
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn close(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn kernel_angles_recompose_every_proper_rotation_including_gimbal_lock() {
    for angles in [
        [0.0, 0.0, 0.0],
        [90.0, 0.0, 0.0],
        [-90.0, 0.0, 0.0],
        [0.0, 30.0, 0.0],
        [0.0, 0.0, 90.0],
        [90.0, 30.0, 0.0],
        [0.0, 30.0, 90.0],
        [90.0, 30.0, 90.0],
        [20.0, -35.0, 110.0],
        [-150.0, 75.0, 40.0],
        [35.0, 90.0, 0.0],
        [35.0, -90.0, 0.0],
        [35.0, 90.0, 20.0],
    ] {
        let matrix = rotation(angles);
        let recovered = placement::rotation_deg(matrix);
        assert!(
            close(rotation(recovered), matrix),
            "{angles:?} -> {recovered:?}"
        );
    }
    let fin = placement::rotation_deg([[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]]);
    assert!((fin[0] - 90.0).abs() < 1e-9 && fin[1].abs() < 1e-9 && fin[2].abs() < 1e-9);
}

#[test]
fn symmetric_surfaces_reject_lateral_offsets_and_tilted_span_axes() {
    let level = Frame {
        rotation: rotation([0.0, 0.0, 0.0]),
        translation: [12.0, 0.0, -1.0],
    };
    assert!(placement::check_symmetric("wing", &level).is_ok());
    let inverted = Frame {
        rotation: rotation([180.0, 0.0, 0.0]),
        translation: [12.0, 0.0, -1.0],
    };
    assert!(placement::check_symmetric("wing", &inverted).is_ok());
    let offset = Frame {
        rotation: rotation([0.0, 0.0, 0.0]),
        translation: [12.0, 0.5, -1.0],
    };
    assert!(placement::check_symmetric("wing", &offset).is_err());
    let tilted = Frame {
        rotation: rotation([90.0, 0.0, 0.0]),
        translation: [12.0, 0.0, -1.0],
    };
    assert!(placement::check_symmetric("wing", &tilted).is_err());
}

fn concept_model() -> VehicleModel {
    serde_json::from_str(include_str!(
        "../../../../examples/synthesis/kernel-transport.json"
    ))
    .unwrap()
}

fn native_inputs(model: &VehicleModel) -> (AnalysisResources, BTreeMap<String, Quantity>) {
    let mut session =
        super::super::create_session_blocking(model.clone(), &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    let values = snapshot
        .values
        .iter()
        .filter_map(|(name, record)| record.quantity.map(|quantity| (name.clone(), quantity)))
        .collect();
    let resources = AnalysisResources::collect(model, &model.analyses["kernel_geometry"]).unwrap();
    (resources, values)
}

#[test]
fn concept_shapes_resolve_to_placed_kernel_objects() {
    let model = concept_model();
    let (resources, values) = native_inputs(&model);
    let objects = resolve_objects(&resources, &values).unwrap();
    let ids: Vec<_> = objects.iter().map(|object| object.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "engine_left",
            "engine_right",
            "fin",
            "fuselage",
            "tail",
            "wing"
        ]
    );
    let fin = objects.iter().find(|object| object.id == "fin").unwrap();
    assert!((fin.rotation_deg[0] - 90.0).abs() < 1e-9);
    assert!(matches!(
        fin.shape,
        KernelShape::Wing {
            symmetric: false,
            ..
        }
    ));
    let wing = objects.iter().find(|object| object.id == "wing").unwrap();
    assert_eq!(wing.location_m[0], values["wing.root_x"].value);
    assert_eq!(wing.location_m[1], 0.0);
    assert!(matches!(
        wing.shape,
        KernelShape::Wing {
            symmetric: true,
            span_m,
            ..
        } if span_m == values["wing.span"].value
    ));
    let nacelles: Vec<_> = objects
        .iter()
        .filter(|object| object.id.starts_with("engine_"))
        .map(|object| object.location_m[1])
        .collect();
    assert_eq!(nacelles[0], -nacelles[1]);
    assert!(matches!(
        objects
            .iter()
            .find(|object| object.id == "fuselage")
            .unwrap()
            .shape,
        KernelShape::Body { .. }
    ));
}

fn obj_extents(obj: &str) -> ([f64; 3], [f64; 3]) {
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for line in obj.lines().filter(|line| line.starts_with("v ")) {
        let point: Vec<f64> = line
            .split_whitespace()
            .skip(1)
            .map(|value| value.parse().unwrap())
            .collect();
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    (minimum, maximum)
}

#[test]
fn installed_openvsp_kernel_reproduces_interchange_artifacts_within_native_extents() {
    let backends = BackendRegistry::detect_blocking();
    let Ok(backend) = backends.openvsp() else {
        tracing::warn!("OpenVSP unavailable; kernel geometry integration test skipped");
        return;
    };
    let model = concept_model();
    let (resources, values) = native_inputs(&model);
    let plugin = KernelPlugin::new_blocking(backend.clone()).unwrap();
    let first = plugin
        .evaluate_blocking(&resources, &values, &RunControl::default())
        .unwrap();
    let second = plugin
        .evaluate_blocking(&resources, &values, &RunControl::default())
        .unwrap();
    assert!(first.approximate);
    assert_eq!(first.artifacts.len(), 3);
    for (artifact, repeated) in first.artifacts.iter().zip(&second.artifacts) {
        artifact.validate().unwrap();
        assert_eq!(artifact.digest, repeated.digest, "{}", artifact.schema);
    }
    let media: Vec<_> = first
        .artifacts
        .iter()
        .map(|artifact| artifact.media_type.as_str())
        .collect();
    assert_eq!(media, ["application/xml", "model/step", "model/obj"]);
    let step = first.artifacts[1].text.as_deref().unwrap();
    assert!(step.contains("FILE_NAME('outfile.stp','2000-01-01T00:00:00'"));
    assert!(step.contains("CONFIG_CONTROL_DESIGN"));
    assert!(
        first.artifacts[0]
            .text
            .as_deref()
            .unwrap()
            .contains("ACE_fuselage")
    );
    let components = first.details["components"].as_array().unwrap();
    assert_eq!(components.len(), 6, "{components:?}");
    let wetted = first.values["wetted_area"].value;
    let volume = first.values["volume"].value;
    assert!(wetted > 0.0 && volume > 0.0);
    let native = ace_physics::evaluate_concept_geometry(&resources, &values).unwrap();
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for object in native.objects.values() {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(object.minimum_m[axis]);
            maximum[axis] = maximum[axis].max(object.maximum_m[axis]);
        }
    }
    let (kernel_minimum, kernel_maximum) = obj_extents(first.artifacts[2].text.as_deref().unwrap());
    for axis in 0..3 {
        let tolerance = 0.02 * (maximum[axis] - minimum[axis]) + 0.05;
        assert!(
            (kernel_minimum[axis] - minimum[axis]).abs() < tolerance
                && (kernel_maximum[axis] - maximum[axis]).abs() < tolerance,
            "axis {axis}: kernel [{}, {}] native [{}, {}]",
            kernel_minimum[axis],
            kernel_maximum[axis],
            minimum[axis],
            maximum[axis]
        );
    }
    assert!((wetted - native.part_area_m2).abs() < 0.25 * native.part_area_m2);
}

#[test]
fn installed_openvsp_kernel_publishes_through_a_synthesis_session() {
    if BackendRegistry::detect_blocking().openvsp().is_err() {
        return;
    }
    let mut session =
        super::super::create_session_blocking(concept_model(), &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    assert!(snapshot.complete());
    let run = snapshot
        .runs
        .iter()
        .find(|run| run.plugin.as_ref().is_some_and(|plugin| plugin.id == ID))
        .unwrap();
    assert_eq!(run.artifacts.len(), 3);
    assert!(
        snapshot.values["geometry.kernel_wetted_area"]
            .quantity
            .unwrap()
            .value
            > 0.0
    );
    assert!(
        snapshot.values["geometry.kernel_volume"]
            .quantity
            .unwrap()
            .value
            > 0.0
    );
}
