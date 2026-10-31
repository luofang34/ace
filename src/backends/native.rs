use crate::backends::contracts::{BackendDescriptor, GeometryBackend};

mod descriptor;

use descriptor::native_descriptor;

/// The in-process conceptual models, always available.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeBackend;

impl GeometryBackend for NativeBackend {
    fn descriptor(&self) -> BackendDescriptor {
        native_descriptor()
    }
}
