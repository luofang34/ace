use crate::{
    Analysis, Binding, Clearance, GeometryObject, LoadingCase, MassItem, Mission, ModelError,
    ModelResult, Operations, Section, VehicleModel,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod frames;

/// Non-scalar model objects explicitly consumed by one analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceSelection {
    /// Clearance checks and their two semantic objects.
    #[serde(default)]
    pub clearances: Vec<String>,
    /// Requested sections and their semantic objects.
    #[serde(default)]
    pub sections: Vec<String>,
    /// Mass contributions included in an aggregation.
    #[serde(default)]
    pub mass_items: Vec<String>,
    /// Loading cases and the complete vehicle mass distribution.
    #[serde(default)]
    pub loading_cases: Vec<String>,
    /// Missions, their phase analyses, and complete vehicle mass definitions.
    #[serde(default)]
    pub missions: Vec<String>,
    /// Whether the vehicle's operating case is consumed.
    #[serde(default)]
    pub operations: bool,
    /// Whether the vehicle's production bill is consumed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub production: bool,
    /// Catalog entries interpreted by the solver.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalog: Vec<String>,
}

/// Exact declarative resources retained in calculation evidence.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnalysisResources {
    /// Semantic geometry declarations.
    pub geometry: BTreeMap<String, GeometryObject>,
    /// Referenced coordinate frames, excluded from selected geometry measurements.
    #[serde(default)]
    pub frames: BTreeMap<String, GeometryObject>,
    /// Pairwise required separations.
    pub clearances: BTreeMap<String, Clearance>,
    /// User-selected structural or packaging stations.
    pub sections: BTreeMap<String, Section>,
    /// Non-overlapping mass contributions.
    pub mass_items: BTreeMap<String, MassItem>,
    /// Loading distributions.
    pub loading_cases: BTreeMap<String, LoadingCase>,
    /// Mission declarations.
    pub missions: BTreeMap<String, Mission>,
    /// Phase calculations whose outputs drive the state engine.
    pub stage_analyses: BTreeMap<String, Analysis>,
    /// Selected operating case.
    pub operations: Option<Operations>,
    /// Selected production bill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production: Option<crate::Production>,
    /// Selected catalog entries.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub catalog: BTreeMap<String, crate::CatalogEntry>,
}

impl AnalysisResources {
    /// Resolve selected objects, including references required to interpret them.
    pub fn collect(model: &VehicleModel, analysis: &Analysis) -> ModelResult<Self> {
        if analysis.mode == crate::AnalysisMode::Phase && !analysis.resources.missions.is_empty() {
            return Err(ModelError::invalid(
                &analysis.plugin,
                "a phase cannot recursively select missions",
            ));
        }
        let mut resources = Self::default();
        select(&model.geometry, &analysis.geometry, &mut resources.geometry)?;
        select(
            &model.clearances,
            &analysis.resources.clearances,
            &mut resources.clearances,
        )?;
        select(
            &model.sections,
            &analysis.resources.sections,
            &mut resources.sections,
        )?;
        select(
            &model.mass_items,
            &analysis.resources.mass_items,
            &mut resources.mass_items,
        )?;
        select(
            &model.loading_cases,
            &analysis.resources.loading_cases,
            &mut resources.loading_cases,
        )?;
        select(
            &model.missions,
            &analysis.resources.missions,
            &mut resources.missions,
        )?;
        for mission in resources.missions.values() {
            resources.mass_items.extend(model.mass_items.clone());
            if let Some(id) = &mission.loading {
                select(
                    &model.loading_cases,
                    std::slice::from_ref(id),
                    &mut resources.loading_cases,
                )?;
            }
            for stage in mission.all_stages() {
                select(
                    &model.analyses,
                    std::slice::from_ref(&stage.analysis),
                    &mut resources.stage_analyses,
                )?;
            }
        }
        for case in resources.loading_cases.values() {
            select(
                &model.mass_items,
                &case.masses.keys().cloned().collect::<Vec<_>>(),
                &mut resources.mass_items,
            )?;
        }
        if !resources.loading_cases.is_empty() {
            resources.mass_items.extend(model.mass_items.clone());
        }
        if analysis.resources.production {
            let production = model.production.clone().ok_or_else(|| {
                ModelError::invalid(
                    "analysis.resources.production",
                    "no production bill is defined",
                )
            })?;
            let items: Vec<_> = production.mass_items().map(str::to_owned).collect();
            select(&model.mass_items, &items, &mut resources.mass_items)?;
            resources.production = Some(production);
        }
        select(
            &model.catalog,
            &analysis.resources.catalog,
            &mut resources.catalog,
        )?;
        resources.include_geometry(&model.geometry)?;
        resources.include_phases(model)?;
        resources.include_frames(&model.geometry)?;
        if analysis.resources.operations {
            resources.operations = Some(model.operations.clone().ok_or_else(|| {
                ModelError::invalid(
                    "analysis.resources.operations",
                    "no operating case is defined",
                )
            })?);
        }
        Ok(resources)
    }

    fn include_geometry(&mut self, geometry: &BTreeMap<String, GeometryObject>) -> ModelResult<()> {
        let mut geometry_ids = BTreeSet::new();
        for clearance in self.clearances.values() {
            geometry_ids.extend([clearance.first.clone(), clearance.second.clone()]);
        }
        for section in self.sections.values() {
            geometry_ids.extend(section.geometry.iter().cloned());
        }
        for mass in self.mass_items.values() {
            if let Some(id) = &mass.geometry {
                geometry_ids.insert(id.clone());
            }
        }
        select(
            geometry,
            &geometry_ids.into_iter().collect::<Vec<_>>(),
            &mut self.geometry,
        )?;
        Ok(())
    }

    fn include_phases(&mut self, model: &VehicleModel) -> ModelResult<()> {
        let phases: Vec<_> = self
            .stage_analyses
            .values()
            .filter(|analysis| analysis.mode == crate::AnalysisMode::Phase)
            .cloned()
            .collect();
        for analysis in phases {
            let phase = Self::collect(model, &analysis)?;
            self.geometry.extend(phase.geometry);
            self.frames.extend(phase.frames);
            self.clearances.extend(phase.clearances);
            self.sections.extend(phase.sections);
            self.mass_items.extend(phase.mass_items);
            self.loading_cases.extend(phase.loading_cases);
            self.catalog.extend(phase.catalog);
            if phase.operations.is_some() {
                self.operations = phase.operations;
            }
        }
        Ok(())
    }

    /// Restrict a phase invocation to its declared semantic resources.
    pub fn for_phase(&self, analysis: &Analysis) -> ModelResult<Self> {
        if analysis.mode != crate::AnalysisMode::Phase || !analysis.resources.missions.is_empty() {
            return Err(ModelError::invalid(
                &analysis.plugin,
                "expected a nonrecursive phase",
            ));
        }
        let mut result = Self::default();
        select(&self.geometry, &analysis.geometry, &mut result.geometry)?;
        select(
            &self.clearances,
            &analysis.resources.clearances,
            &mut result.clearances,
        )?;
        select(
            &self.sections,
            &analysis.resources.sections,
            &mut result.sections,
        )?;
        select(
            &self.mass_items,
            &analysis.resources.mass_items,
            &mut result.mass_items,
        )?;
        select(
            &self.loading_cases,
            &analysis.resources.loading_cases,
            &mut result.loading_cases,
        )?;
        select(
            &self.catalog,
            &analysis.resources.catalog,
            &mut result.catalog,
        )?;
        if !result.loading_cases.is_empty() {
            result.mass_items.extend(self.mass_items.clone());
        }
        let mut geometry = self.frames.clone();
        geometry.extend(self.geometry.clone());
        result.include_geometry(&geometry)?;
        result.include_frames(&geometry)?;
        if analysis.resources.operations {
            result.operations = self.operations.clone();
        }
        Ok(result)
    }

    /// All canonical values used by selected resources, including phase outputs.
    pub fn dependencies(&self) -> BTreeSet<String> {
        let mut dependencies = BTreeSet::new();
        for binding in self.bindings() {
            if let Some(name) = binding.parameter() {
                dependencies.insert(name.to_owned());
            }
        }
        for analysis in self.stage_analyses.values() {
            let ports = if analysis.mode == crate::AnalysisMode::Phase {
                &analysis.inputs
            } else {
                &analysis.outputs
            };
            dependencies.extend(ports.values().cloned());
        }
        dependencies
    }

    fn bindings(&self) -> Vec<&Binding> {
        let mut values = Vec::new();
        for geometry in self.geometry.values().chain(self.frames.values()) {
            values.extend(geometry.bindings().into_iter().map(|(value, _)| value));
        }
        values.extend(self.clearances.values().map(|item| &item.minimum));
        values.extend(self.sections.values().map(|item| &item.station));
        for item in self.mass_items.values() {
            values.push(&item.mass);
            values.extend(&item.cg);
            values.extend(item.capacity.iter());
            if let Some(inertia) = &item.inertia {
                values.extend([
                    &inertia.xx,
                    &inertia.yy,
                    &inertia.zz,
                    &inertia.xy,
                    &inertia.xz,
                    &inertia.yz,
                ]);
            }
            for station in &item.fill_curve {
                values.extend(&station.cg);
            }
        }
        for case in self.loading_cases.values() {
            values.extend(case.masses.values());
        }
        for mission in self.missions.values() {
            values.extend(mission.conditions.values());
            for limit in &mission.cg_envelope {
                values.extend([&limit.mass, &limit.minimum, &limit.maximum]);
            }
            for stage in mission.all_stages() {
                values.extend(stage.inputs.values());
                values.extend(stage.payload_release.values());
            }
        }
        if let Some(operations) = &self.operations {
            values.extend(operations.bindings().into_iter().map(|(value, _)| value));
        }
        if let Some(production) = &self.production {
            values.extend(production.bindings().into_iter().map(|(value, _)| value));
        }
        values
    }
}

fn select<T: Clone>(
    source: &BTreeMap<String, T>,
    ids: &[String],
    target: &mut BTreeMap<String, T>,
) -> ModelResult<()> {
    for id in ids {
        let value = source
            .get(id)
            .ok_or_else(|| ModelError::UnknownReference(id.clone()))?;
        target.insert(id.clone(), value.clone());
    }
    Ok(())
}
