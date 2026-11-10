//! Compiles a design file into a system package directory:
//! `cargo run -p ace-export --example export_syspack -- <vehicle.json> <dir>`.

use std::error::Error;
use std::path::Path;

use ace_export::{AceCompileOptions, compile_ace_vehicle_to_syspack, write_syspack_to_dir};
use ace_model::VehicleModel;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let (Some(design), Some(output)) = (arguments.next(), arguments.next()) else {
        return Err("usage: export_syspack <vehicle.json> <output-dir>".into());
    };
    let model: VehicleModel = serde_json::from_slice(&std::fs::read(design)?)?;
    let source = compile_ace_vehicle_to_syspack(&model, &AceCompileOptions::default())?;
    write_syspack_to_dir(&source, Path::new(&output))?;
    Ok(())
}
