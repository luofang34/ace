//! Bounded vehicle design studies with independent objectives and shared calculation evidence.

mod error;
mod evaluation;
mod evidence;
mod plan;
mod ranking;
mod record;
mod search;

pub use error::{StudyError, StudyResult};
pub use plan::{SearchMethod, StudyPlan};
pub use record::{
    Candidate, CandidateState, Feasibility, ObjectiveValue, StopReason, StudyArchive,
};
pub use search::run;
