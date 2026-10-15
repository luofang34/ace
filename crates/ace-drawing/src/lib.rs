//! Replayable engineering drawings projected from verified vehicle synthesis evidence.

mod document;
mod error;
mod layout;
mod render;
mod scene;

pub use document::{DrawingDocument, DrawingInput, Paper, compose};
pub use error::{DrawingError, DrawingResult};
pub use scene::{Element, Evidence, Primitive, Scene, Stroke};
