use syspack::{DirectoryPackageSource, SyspackError};

use super::*;

const VEHICLE_JSON: &str = include_str!("../../../../examples/c172/vehicle.json");

fn c172() -> VehicleModel {
    serde_json::from_str(VEHICLE_JSON).unwrap()
}

fn compile(model: &VehicleModel, options: &AceCompileOptions) -> SyspackManifest {
    let source = compile_ace_vehicle_to_syspack(model, options).unwrap();
    SyspackManifest::parse(&source.read_manifest_bytes().unwrap()).unwrap()
}

fn component<'a>(manifest: &'a SyspackManifest, key: &str) -> &'a ComponentRecord {
    manifest.components.iter().find(|c| c.key == key).unwrap()
}

#[test]
fn model_stations_become_root_axes() {
    let root = root_from_station(DVec3::new(3.0, 0.5, 0.2));
    assert_eq!(root, DVec3::new(-3.0, 0.5, -0.2));
}

#[test]
fn the_tree_hangs_from_the_airframe_at_the_empty_centre_of_gravity() {
    let manifest = compile(&c172(), &AceCompileOptions::default());
    assert_eq!(
        manifest.components[0].key, AIRFRAME,
        "the root is component 0"
    );
    let airframe = component(&manifest, AIRFRAME);
    assert_eq!(airframe.parent, None);
    assert_eq!(airframe.translation_m, [-3.05, 0.0, 0.0]);
    assert_eq!(airframe.dry_mass_kg, 767.0);
    let tank = component(&manifest, "fuel_tank");
    assert_eq!(tank.parent.as_deref(), Some(AIRFRAME));
    assert!(
        (tank.translation_m[0] + 0.1).abs() < 1e-12,
        "aft of the airframe"
    );
    assert_eq!(tank.initial_fluid_mass_kg, 114.0);
    let payload = component(&manifest, "payload_station");
    assert!((payload.translation_m[0] + 0.2).abs() < 1e-12);
    assert_eq!(payload.initial_payload_mass_kg, 230.0);
    let camera = component(&manifest, "fpv_camera_mount");
    assert_eq!(
        camera.translation_m,
        [2.0, 0.0, 0.0],
        "forward of the airframe"
    );
}

#[test]
fn the_camera_declaration_and_its_mount_agree() {
    let manifest = compile(&c172(), &AceCompileOptions::default());
    let camera = manifest
        .devices
        .iter()
        .find(|d| d.id == "fpv_camera")
        .unwrap();
    let DeviceKindSpec::PinholeCamera { offset_body_m, .. } = camera.kind else {
        panic!("{:?}", camera.kind);
    };
    let mount = component(&manifest, &camera.site);
    let airframe = component(&manifest, AIRFRAME);
    let root_of_mount = DVec3::from(airframe.translation_m) + DVec3::from(mount.translation_m);
    assert_eq!(DVec3::from(offset_body_m), root_of_mount);
}

#[test]
fn the_declared_digest_is_the_assets_own() {
    let source = compile_ace_vehicle_to_syspack(&c172(), &AceCompileOptions::default()).unwrap();
    let manifest = SyspackManifest::parse(&source.read_manifest_bytes().unwrap()).unwrap();
    let (path, declared) = manifest.assets.iter().next().unwrap();
    assert_eq!(path, "fdm/c172_class_reference.xml");
    let bytes = source.read_asset_bytes(path).unwrap();
    assert_eq!(declared, &digest_of(&bytes));
    assert_eq!(declared.size_bytes, bytes.len() as u64);
    assert_eq!(declared.sha256.len(), 64);
}

#[test]
fn the_jsbsim_paths_are_where_jsbsim_will_look() {
    let manifest = compile(&c172(), &AceCompileOptions::default());
    let DynamicsSpec::JsbsimXml {
        model_name,
        virtual_path,
        ..
    } = manifest.dynamics
    else {
        panic!("not JSBSim");
    };
    assert_eq!(
        virtual_path,
        format!("aircraft/{model_name}/{model_name}.xml")
    );
}

#[test]
fn a_hyphenated_id_gets_a_jsbsim_name_of_its_own() {
    let mut model = c172();
    model.id = "c172-class".to_owned();
    let manifest = compile(&model, &AceCompileOptions::default());
    assert_eq!(manifest.id, "c172-class");
    let DynamicsSpec::JsbsimXml { model_name, .. } = manifest.dynamics else {
        panic!("not JSBSim");
    };
    assert_eq!(model_name, "c172_class");
}

#[test]
fn options_carry_into_the_manifest() {
    let mut model = c172();
    model.revision = 12;
    let defaulted = compile(&model, &AceCompileOptions::default());
    assert_eq!(defaulted.version, "0.0.12");
    let options = AceCompileOptions {
        version: Some("2.1.0".to_owned()),
        imu_seed: 77,
    };
    let chosen = compile(&model, &options);
    assert_eq!(chosen.version, "2.1.0");
    assert_eq!(
        chosen.devices[0].kind,
        DeviceKindSpec::Icm42688 { seed: 77 }
    );
}

#[test]
fn an_id_the_loader_would_refuse_never_leaves_the_compiler() {
    let mut model = c172();
    model.id = "Not A Slug".to_owned();
    let error = compile_ace_vehicle_to_syspack(&model, &AceCompileOptions::default()).unwrap_err();
    assert!(
        matches!(
            error,
            AceExportError::PackageRejected(SyspackError::InvalidPackageId { .. })
        ),
        "{error}"
    );
}

#[test]
fn every_device_is_on_a_component_the_tree_has() {
    let source = compile_ace_vehicle_to_syspack(&c172(), &AceCompileOptions::default()).unwrap();
    let package = VerifiedSyspack::load(&source).unwrap();
    for device in &package.manifest().devices {
        package.component_id(&device.site).unwrap();
    }
}

#[test]
fn a_piston_design_declares_its_cockpit() {
    let manifest = compile(&c172(), &AceCompileOptions::default());
    assert_eq!(manifest.kind, SystemKind::FixedWing);
    assert_eq!(
        manifest.controls,
        ControlsSpec {
            has_magnetos: true,
            has_starter: true,
            has_mixture: true,
            has_fuel_cutoff: true,
            has_flaps: true,
            has_wheel_brakes: true,
            ..ControlsSpec::default()
        }
    );
}

#[test]
fn a_design_with_no_piston_engine_declares_no_ignition() {
    let mut model = c172();
    model.catalog.get_mut("engine").unwrap().kind = "electric_motor".to_owned();
    let manifest = compile(&model, &AceCompileOptions::default());
    assert_eq!(
        manifest.controls,
        ControlsSpec {
            has_flaps: true,
            has_wheel_brakes: true,
            ..ControlsSpec::default()
        }
    );
}

#[test]
fn a_written_package_loads_back_as_the_same_system() {
    let source = compile_ace_vehicle_to_syspack(&c172(), &AceCompileOptions::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nested/c172");
    write_syspack_to_dir(&source, &target).unwrap();
    let written = VerifiedSyspack::load(&DirectoryPackageSource::new(&target)).unwrap();
    assert_eq!(written, VerifiedSyspack::load(&source).unwrap());
    assert!(target.join("syspack.json").is_file());
    assert!(target.join("fdm/c172_class_reference.xml").is_file());
}

#[test]
fn a_package_that_does_not_verify_is_not_written() {
    let source = compile_ace_vehicle_to_syspack(&c172(), &AceCompileOptions::default()).unwrap();
    let tampered = MemoryPackageSource::new(source.read_manifest_bytes().unwrap(), BTreeMap::new());
    let dir = tempfile::tempdir().unwrap();
    let error = write_syspack_to_dir(&tampered, dir.path()).unwrap_err();
    assert!(
        matches!(error, AceExportError::PackageRejected(_)),
        "{error}"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
