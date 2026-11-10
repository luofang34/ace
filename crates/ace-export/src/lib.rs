//! Compiles vehicle designs into universal system packages.
//!
//! [`compile_ace_vehicle_to_syspack`] reads a fixed-wing
//! [`ace_model::VehicleModel`], synthesises a component tree and a JSBSim
//! aircraft from it, and emits a package that has already passed
//! [`syspack::VerifiedSyspack::load`]; [`write_syspack_to_dir`] puts one on
//! disk. [`capsule_dynamics_from_solution`] turns an `ace-cfd` blunt-body
//! solution into the capsule model's coefficients.
//!
//! A conceptual design says less than a flight model needs, so everything
//! `ace-rs` does not state is a named, representative estimate: see
//! [`FixedWingDesign`] for what comes from the model and what does not.

mod aerothermal;
mod design;
mod error;
mod jsbsim_xml;
mod package;

pub use aerothermal::{
    CapsuleFreestream, CapsuleGeometry, capsule_dynamics_from_solution, capsule_params_from_wall,
};
pub use design::{
    FixedWingDesign, FlapDesign, MassStation, PistonDesign, Powerplant, PropellerDesign,
};
pub use error::AceExportError;
pub use jsbsim_xml::jsbsim_aircraft_xml;
pub use package::{AceCompileOptions, compile_ace_vehicle_to_syspack, write_syspack_to_dir};
