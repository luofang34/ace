use crate::backends::contracts::BackendDescriptor;
use crate::services::analysis::ApplicationService;

impl ApplicationService {
    pub(crate) fn list_analysis_backends(&self) -> Vec<BackendDescriptor> {
        self.backends.descriptors()
    }
}
