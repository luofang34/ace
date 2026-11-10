use std::collections::BTreeMap;

use ace_model::VehicleModel;
use std::path::Path;

use glam::{DMat3, DVec3};
use sha2::{Digest, Sha256};
use syspack::{
    AssetDigest, ComponentRecord, ControlsSpec, DeviceKindSpec, DynamicsSpec, MANIFEST_FILE,
    MemoryPackageSource, MountedDeviceSpec, PackageSource, SCHEMA_VERSION, SyspackManifest,
    SystemKind, VerifiedSyspack,
};

use crate::jsbsim_xml::nose_station_m;
use crate::{AceExportError, FixedWingDesign, Powerplant, jsbsim_aircraft_xml};

#[cfg(test)]
mod tests;

const AIRCRAFT_DIR: &str = "aircraft";
const FDM_ASSET_DIR: &str = "fdm";
const BMP390_DEFAULT_ADDRESS: u8 = 0x76;
const NO_ROTATION: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
/// Body-to-camera for a camera looking along the nose, right side up.
const FORWARD_CAMERA_XYZW: [f64; 4] = [0.5, 0.5, -0.5, 0.5];
/// Where the avionics sit against the empty centre of gravity, root axes,
/// metres: a little ahead and above, as a panel-mounted unit is.
const AVIONICS_OFFSET_M: DVec3 = DVec3::new(0.3, 0.0, -0.2);

/// The choices a package makes that a design does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AceCompileOptions {
    /// The package's version; the model's revision unless overridden.
    pub version: Option<String>,
    /// Seed of the IMU's noise stream.
    pub imu_seed: u64,
}

impl Default for AceCompileOptions {
    fn default() -> Self {
        Self {
            version: None,
            imu_seed: 1,
        }
    }
}

/// The model's station in the package's root axes: `x` forward, `y` right, `z`
/// down, with the root origin at the model's datum.
fn root_from_station(station_m: DVec3) -> DVec3 {
    DVec3::new(-station_m.x, station_m.y, -station_m.z)
}

struct Component<'a> {
    key: &'a str,
    name: &'a str,
    translation_m: DVec3,
    fluid_kg: f64,
    payload_kg: f64,
}

impl Component<'_> {
    /// A child of the airframe holding nothing of its own.
    fn site<'a>(key: &'a str, name: &'a str, translation_m: DVec3) -> Component<'a> {
        Component {
            key,
            name,
            translation_m,
            fluid_kg: 0.0,
            payload_kg: 0.0,
        }
    }

    fn record(&self) -> ComponentRecord {
        ComponentRecord {
            key: self.key.to_owned(),
            name: self.name.to_owned(),
            parent: Some(AIRFRAME.to_owned()),
            translation_m: self.translation_m.to_array(),
            rotation_xyzw: NO_ROTATION,
            dry_mass_kg: 0.0,
            dry_inertia_body: matrix(DMat3::ZERO),
            initial_fluid_mass_kg: self.fluid_kg,
            initial_payload_mass_kg: self.payload_kg,
            attached: true,
            thermal_capacity_j_k: 0.0,
            nominal_power_w: 0.0,
        }
    }
}

const AIRFRAME: &str = "airframe";

fn matrix(rows: DMat3) -> [[f64; 3]; 3] {
    rows.to_cols_array_2d()
}

/// The tree: an airframe at the empty centre of gravity with the rest hung
/// from it, each offset measured from that point.
fn components(design: &FixedWingDesign) -> Vec<ComponentRecord> {
    let airframe_at = root_from_station(design.empty.station_m);
    let offset = |station_m| root_from_station(station_m) - airframe_at;
    let airframe = ComponentRecord {
        key: AIRFRAME.to_owned(),
        name: "Airframe".to_owned(),
        parent: None,
        translation_m: airframe_at.to_array(),
        rotation_xyzw: NO_ROTATION,
        dry_mass_kg: design.empty.mass_kg,
        dry_inertia_body: matrix(design.empty_inertia_kg_m2),
        initial_fluid_mass_kg: 0.0,
        initial_payload_mass_kg: 0.0,
        attached: true,
        thermal_capacity_j_k: 0.0,
        nominal_power_w: 0.0,
    };
    let children = [
        Component {
            fluid_kg: design.fuel.mass_kg,
            ..Component::site("fuel_tank", "Fuel tank", offset(design.fuel.station_m))
        },
        Component {
            payload_kg: design.payload.mass_kg,
            ..Component::site(
                "payload_station",
                "Payload station",
                offset(design.payload.station_m),
            )
        },
        Component::site("imu_mount", "IMU mount", AVIONICS_OFFSET_M),
        Component::site("baro_mount", "Barometer mount", AVIONICS_OFFSET_M),
        Component::site(
            "fpv_camera_mount",
            "FPV camera mount",
            offset(nose_station_m(design)),
        ),
    ];
    std::iter::once(airframe)
        .chain(children.iter().map(Component::record))
        .collect()
}

fn devices(design: &FixedWingDesign, options: &AceCompileOptions) -> Vec<MountedDeviceSpec> {
    let device = |id: &str, site: &str, kind| MountedDeviceSpec {
        id: id.to_owned(),
        site: site.to_owned(),
        kind,
    };
    vec![
        device(
            "imu",
            "imu_mount",
            DeviceKindSpec::Icm42688 {
                seed: options.imu_seed,
            },
        ),
        device(
            "barometer",
            "baro_mount",
            DeviceKindSpec::Bmp390 {
                bus_id: 0,
                i2c_addr: BMP390_DEFAULT_ADDRESS,
            },
        ),
        device(
            "fpv_camera",
            "fpv_camera_mount",
            DeviceKindSpec::PinholeCamera {
                offset_body_m: root_from_station(nose_station_m(design)).to_array(),
                body_to_camera_xyzw: FORWARD_CAMERA_XYZW,
                width: 800,
                height: 600,
                fx: 600.0,
                fy: 600.0,
                cx: 399.5,
                cy: 299.5,
                rate_hz: 30,
            },
        ),
    ]
}

/// What the cockpit has: every design gets flaps and wheel brakes, and a
/// piston engine brings its ignition, starter, mixture and fuel shutoff.
fn controls(design: &FixedWingDesign) -> ControlsSpec {
    let piston = matches!(design.powerplant, Powerplant::Piston(_));
    ControlsSpec {
        has_magnetos: piston,
        has_starter: piston,
        has_mixture: piston,
        has_prop_pitch: false,
        has_fuel_cutoff: piston,
        has_flaps: true,
        has_retractable_gear: false,
        has_wheel_brakes: true,
    }
}

fn digest_of(bytes: &[u8]) -> AssetDigest {
    AssetDigest {
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        size_bytes: bytes.len() as u64,
    }
}

/// Compiles `model` into a vehicle package and proves it by loading it, so an
/// `Ok` is a package [`VerifiedSyspack::load`] accepts.
pub fn compile_ace_vehicle_to_syspack(
    model: &VehicleModel,
    options: &AceCompileOptions,
) -> Result<MemoryPackageSource, AceExportError> {
    let design = FixedWingDesign::from_model(model)?;
    let model_name = design.id.replace('-', "_");
    let xml = jsbsim_aircraft_xml(&design, &model_name)?.into_bytes();
    let entry_asset = format!("{FDM_ASSET_DIR}/{model_name}.xml");
    let manifest = SyspackManifest {
        schema_version: SCHEMA_VERSION,
        id: design.id.clone(),
        name: design.name.clone(),
        version: options
            .version
            .clone()
            .unwrap_or_else(|| format!("0.0.{}", model.revision)),
        kind: SystemKind::FixedWing,
        assets: BTreeMap::from([(entry_asset.clone(), digest_of(&xml))]),
        components: components(&design),
        dynamics: DynamicsSpec::JsbsimXml {
            aircraft_dir: AIRCRAFT_DIR.to_owned(),
            virtual_path: format!("{AIRCRAFT_DIR}/{model_name}/{model_name}.xml"),
            model_name,
            entry_asset: entry_asset.clone(),
            additional_assets: Vec::new(),
        },
        controls: controls(&design),
        devices: devices(&design, options),
        instrument_pack: None,
        parts: Vec::new(),
        typeface: None,
    };
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(AceExportError::ManifestEncode)?;
    let source = MemoryPackageSource::new(bytes, BTreeMap::from([(entry_asset, xml)]));
    VerifiedSyspack::load(&source).map_err(AceExportError::PackageRejected)?;
    Ok(source)
}

fn create_parent(file: &Path) -> Result<(), AceExportError> {
    let Some(parent) = file.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| AceExportError::Write {
        path: parent.display().to_string(),
        source,
    })
}

fn write_file(file: &Path, bytes: &[u8]) -> Result<(), AceExportError> {
    create_parent(file)?;
    std::fs::write(file, bytes).map_err(|source| AceExportError::Write {
        path: file.display().to_string(),
        source,
    })
}

/// Unpacks `source` into `dir` as `syspack.json` and its assets, verifying it
/// first, so a package on disk always loads.
pub fn write_syspack_to_dir(source: &dyn PackageSource, dir: &Path) -> Result<(), AceExportError> {
    let package = VerifiedSyspack::load(source).map_err(AceExportError::PackageRejected)?;
    // The written bytes are the verified bytes, so the package keeps the
    // manifest digest it was verified under.
    let manifest = source
        .read_manifest_bytes()
        .map_err(AceExportError::PackageRejected)?;
    write_file(&dir.join(MANIFEST_FILE), &manifest)?;
    for path in package.manifest().assets.keys() {
        let bytes = package.asset(path).ok_or_else(|| {
            AceExportError::PackageRejected(syspack::SyspackError::AssetMissing {
                path: path.clone(),
            })
        })?;
        write_file(&dir.join(path), bytes)?;
    }
    Ok(())
}
