//! Executable semantic geometry, loading, mission-state, and operations models.

mod bindings;
mod concept;
mod error;
mod geometry;
mod mass;
mod mission;
mod operations;
mod plugins;

pub use concept::{
    ConceptGeometry, ConceptObject, SurfacePart, concept_section, concept_section_area,
    evaluate_concept_geometry,
};
pub use error::{PhysicsError, PhysicsResult};
pub use geometry::{
    AreaStation, ClearanceResult, Envelope, GeometryReport, SectionResult, SectionSlice,
    evaluate_geometry,
};
pub use mass::{
    ItemState, LoadingResult, MassReport, MassState, evaluate_mass, mass_state,
    mass_state_in_frames,
};
pub use mission::{
    CgReport, CgStation, CgWitness, FuelSizing, FuelSizingReport, MissionPoint, MissionReport,
    StageResult, evaluate_cg_envelope, evaluate_mission, evaluate_mission_with, size_mission_fuel,
};
pub use operations::{AirportResult, OperationsReport, evaluate_operations};
pub use plugins::register;
