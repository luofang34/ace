use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;

use crate::services::analysis::ApplicationService;

use super::mcp_error;
use super::output::{ObjectOutput, json_output};

pub(super) fn get(service: &ApplicationService) -> Result<Json<ObjectOutput>, ErrorData> {
    json_output(service.capabilities().map_err(mcp_error)?)
}
