use crate::{AnalysisResources, GeometryObject, ModelError, ModelResult};
use std::collections::{BTreeMap, BTreeSet};

impl AnalysisResources {
    pub(super) fn include_frames(
        &mut self,
        geometry: &BTreeMap<String, GeometryObject>,
    ) -> ModelResult<()> {
        let mut pending: Vec<_> = self
            .geometry
            .values()
            .filter_map(|object| object.placement.relative_to.clone())
            .collect();
        pending.extend(
            self.mass_items
                .values()
                .filter_map(|item| item.frame.clone()),
        );
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let object = geometry
                .get(&id)
                .ok_or_else(|| ModelError::UnknownReference(id.clone()))?;
            if !self.geometry.contains_key(&id) {
                self.frames.insert(id, object.clone());
            }
            if let Some(parent) = &object.placement.relative_to {
                pending.push(parent.clone());
            }
        }
        Ok(())
    }
}
