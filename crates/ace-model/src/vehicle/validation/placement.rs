use crate::{ModelError, ModelResult, VehicleModel};
use std::collections::BTreeSet;

pub(super) fn placement_chain(model: &VehicleModel, id: &str) -> ModelResult<()> {
    let mut current = Some(id);
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return Err(ModelError::invalid(id, "coordinate frame cycle"));
        }
        let object = model
            .geometry
            .get(id)
            .ok_or_else(|| ModelError::UnknownReference(id.into()))?;
        current = object.placement.relative_to.as_deref();
    }
    Ok(())
}
