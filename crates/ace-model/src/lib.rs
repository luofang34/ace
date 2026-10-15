//! Vehicle-independent, serializable design data with explicit dimensional contracts.

mod artifact;
mod edit;
mod error;
mod expression;
mod geometry;
mod mass;
mod mission;
mod operations;
mod parameter;
mod requirement;
mod resources;
mod solver;
mod units;
mod vehicle;

pub use artifact::{Artifact, GeneratorIdentity};
pub use edit::ModelEdit;
pub use error::{ModelError, ModelResult};
pub use expression::Expression;
pub use geometry::{
    Bounds, Clearance, ConceptShape, GeometryObject, GeometryRole, Placement, Section,
};
pub use mass::{FillStation, Inertia, LoadingCase, MassItem, MassKind};
pub use mission::{CgLimit, Mission, MissionBranch, MissionStage, VehicleState};
pub use operations::{
    Airport, AnnualUtilization, CostItem, CostRates, EconomicBasis, EconomicModel,
    OperatingScenario, Operations,
};
pub use parameter::{Binding, Parameter, Source, Uncertainty};
pub use requirement::{DesignVariable, Direction, Objective, Priority, Relation, Requirement};
pub use solver::{Analysis, AnalysisMode, Closure, Tolerance};
pub use units::{DimensionalValue, Quantity, Unit};
pub use vehicle::VehicleModel;

pub use resources::{AnalysisResources, ResourceSelection};
