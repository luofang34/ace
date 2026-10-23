//! Bounded vehicle design studies with independent objectives and shared calculation evidence.

mod calibration;
mod uncertainty;

pub use calibration::{
    Agreement, CalibratedParameter, CalibrationCase, CalibrationRecord, MeasurementSource,
    Observation, Residual,
};
pub use uncertainty::{
    InputDistribution, MetricStatistics, ProbabilityStatus, RequirementProbability,
    UncertaintyPlan, UncertaintyRecord, UncertaintySample,
};

mod error;
mod evaluation;
mod evidence;
mod plan;
mod ranking;
mod record;
mod search;
mod variants;
mod verification;

pub use error::{StudyError, StudyResult};
pub use plan::{SearchMethod, StudyPlan};
pub use record::{
    Candidate, CandidateState, Feasibility, ObjectiveValue, StopReason, StudyArchive,
};
pub use search::run;
pub use variants::{VariantCase, VariantMatrix, VariantOption};
pub use verification::{
    ReferenceKind, ReferenceValue, VerificationCase, VerificationCheck, VerificationRecord,
    VerificationReference,
};
