//! Replayable engineering drawings projected from verified vehicle synthesis evidence.

mod branding;
mod document;
mod error;
mod layout;
mod render;
mod scene;

pub use branding::Branding;
pub use document::{DrawingDocument, DrawingInput, Paper, compose, compose_branded};
pub use error::{DrawingError, DrawingResult};
pub use scene::{Element, Evidence, Primitive, Scene, Stroke};
