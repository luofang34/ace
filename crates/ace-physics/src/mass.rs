use crate::bindings::{nonnegative, scalar, vector};
use crate::{PhysicsError, PhysicsResult};
use ace_model::{AnalysisResources, MassItem, MassKind, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod tensor;

/// Resolved mass and position for one component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemState {
    /// Contribution category.
    pub kind: MassKind,
    /// Loaded mass in kilograms.
    pub mass_kg: f64,
    /// Center of mass in vehicle coordinates, metres.
    pub cg_m: [f64; 3],
    /// Intrinsic inertia about this item's CG, kg m².
    pub inertia_kg_m2: Option<[[f64; 3]; 3]>,
    /// Current fill/occupancy fraction, if capacity is declared.
    pub fill_fraction: Option<f64>,
    /// Linked semantic component.
    pub geometry: Option<String>,
}

/// Three-dimensional mass properties for a single loading state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassState {
    /// Total loaded mass in kilograms.
    pub total_mass_kg: f64,
    /// Center of mass in the common vehicle frame.
    pub cg_m: [f64; 3],
    /// Complete tensor about the overall CG; absent when any intrinsic tensor is unknown.
    pub inertia_kg_m2: Option<[[f64; 3]; 3]>,
    /// Resolved individual contributions.
    pub items: BTreeMap<String, ItemState>,
}

/// Named loading evaluation with an optional failure diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadingResult {
    /// Mass properties when this loading case is physically valid.
    pub state: Option<MassState>,
    /// Why an invalid loading case could not be evaluated.
    pub failure: Option<String>,
}

/// Current mass statement and loading-envelope evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassReport {
    /// Default state from authoritative mass bindings.
    pub current: MassState,
    /// Every requested loading case, including failures.
    pub loadings: BTreeMap<String, LoadingResult>,
    /// Forwardmost valid longitudinal CG.
    pub minimum_cg_x_m: f64,
    /// Aftmost valid longitudinal CG.
    pub maximum_cg_x_m: f64,
    /// Loading that produces the forward limit; `None` denotes the current loading.
    pub minimum_cg_x_loading: Option<String>,
    /// Loading that produces the aft limit; `None` denotes the current loading.
    pub maximum_cg_x_loading: Option<String>,
}

/// Evaluate default and named payload/fuel distributions.
pub fn evaluate_mass(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MassReport> {
    let current = mass_state_in_frames(resources, values, &BTreeMap::new())?;
    let mut minimum = current.cg_m[0];
    let mut maximum = current.cg_m[0];
    let mut minimum_loading = None;
    let mut maximum_loading = None;
    let mut loadings = BTreeMap::new();
    for (id, loading) in &resources.loading_cases {
        let overrides = loading
            .masses
            .iter()
            .map(|(id, value)| Ok((id.clone(), value.resolve(values, Unit::Kilogram)?)))
            .collect::<PhysicsResult<BTreeMap<_, _>>>()?;
        let result = match mass_state_in_frames(resources, values, &overrides) {
            Ok(state) => {
                if state.cg_m[0] < minimum {
                    minimum_loading = Some(id.clone());
                }
                if state.cg_m[0] > maximum {
                    maximum_loading = Some(id.clone());
                }
                minimum = minimum.min(state.cg_m[0]);
                maximum = maximum.max(state.cg_m[0]);
                LoadingResult {
                    state: Some(state),
                    failure: None,
                }
            }
            Err(error) => LoadingResult {
                state: None,
                failure: Some(error.to_string()),
            },
        };
        loadings.insert(id.clone(), result);
    }
    Ok(MassReport {
        current,
        loadings,
        minimum_cg_x_m: minimum,
        maximum_cg_x_m: maximum,
        minimum_cg_x_loading: minimum_loading,
        maximum_cg_x_loading: maximum_loading,
    })
}

/// Aggregate a loading distribution with fill-dependent CG and the parallel-axis theorem.
pub fn mass_state(
    items: &BTreeMap<String, MassItem>,
    values: &BTreeMap<String, Quantity>,
    overrides: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MassState> {
    aggregate(items, &AnalysisResources::default(), values, overrides)
}

/// Aggregate geometry-bound mass properties in vehicle coordinates.
pub fn mass_state_in_frames(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
    overrides: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MassState> {
    aggregate(&resources.mass_items, resources, values, overrides)
}

fn aggregate(
    items: &BTreeMap<String, MassItem>,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
    overrides: &BTreeMap<String, Quantity>,
) -> PhysicsResult<MassState> {
    for id in overrides.keys() {
        if !items.contains_key(id) {
            return Err(PhysicsError::invalid(
                id,
                "loading override has no mass item",
            ));
        }
    }
    let mut resolved = BTreeMap::new();
    let mut total = 0.0;
    let mut moment = [0.0; 3];
    for (id, item) in items {
        let mut resolved_item = resolve_item(id, item, values, overrides.get(id))?;
        if let Some(frame) = &item.frame {
            let transform = crate::geometry::frames::resolve(frame, resources, values)?;
            resolved_item.cg_m = transform.point(resolved_item.cg_m)?;
            resolved_item.inertia_kg_m2 = resolved_item
                .inertia_kg_m2
                .map(|inertia| transform.tensor(inertia));
        }
        total += resolved_item.mass_kg;
        for (moment, position) in moment.iter_mut().zip(resolved_item.cg_m) {
            *moment += resolved_item.mass_kg * position;
        }
        resolved.insert(id.clone(), resolved_item);
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(PhysicsError::invalid(
            "mass",
            "total mass must be positive and finite",
        ));
    }
    let cg_m = moment.map(|value| value / total);
    if cg_m.iter().any(|value| !value.is_finite()) {
        return Err(PhysicsError::invalid("mass", "non-finite mass moment"));
    }
    let inertia_kg_m2 = tensor::aggregate(&resolved, cg_m)?;
    Ok(MassState {
        total_mass_kg: total,
        cg_m,
        inertia_kg_m2,
        items: resolved,
    })
}

fn resolve_item(
    id: &str,
    item: &MassItem,
    values: &BTreeMap<String, Quantity>,
    override_mass: Option<&Quantity>,
) -> PhysicsResult<ItemState> {
    let mass = match override_mass {
        Some(value) => value.to(Unit::Kilogram)?.value,
        None => scalar(&item.mass, values, Unit::Kilogram)?,
    };
    nonnegative(id, mass)?;
    let capacity = item
        .capacity
        .as_ref()
        .map(|value| scalar(value, values, Unit::Kilogram))
        .transpose()?;
    if let Some(capacity) = capacity
        && (capacity <= 0.0 || mass > capacity)
    {
        return Err(PhysicsError::invalid(
            id,
            format!("loaded mass {mass} kg exceeds usable capacity {capacity} kg"),
        ));
    }
    let fill_fraction = capacity.map(|capacity| mass / capacity);
    let cg_m = center(item, values, fill_fraction)?;
    let declared_mass = nonnegative(id, scalar(&item.mass, values, Unit::Kilogram)?)?;
    // A changed loading does not establish the new internal mass distribution.
    let inertia_kg_m2 = if mass == declared_mass {
        item.inertia
            .as_ref()
            .map(|inertia| tensor::resolve(inertia, values))
            .transpose()?
    } else {
        None
    };
    Ok(ItemState {
        kind: item.kind,
        mass_kg: mass,
        cg_m,
        inertia_kg_m2,
        fill_fraction,
        geometry: item.geometry.clone(),
    })
}

fn center(
    item: &MassItem,
    values: &BTreeMap<String, Quantity>,
    fill: Option<f64>,
) -> PhysicsResult<[f64; 3]> {
    if item.fill_curve.is_empty() {
        return vector(&item.cg, values);
    }
    let fill = fill
        .ok_or_else(|| PhysicsError::invalid("mass.fill_curve", "fill curve requires capacity"))?;
    for pair in item.fill_curve.windows(2) {
        if let [first, last] = pair {
            if first.fraction >= last.fraction
                || !(0.0..=1.0).contains(&first.fraction)
                || !(0.0..=1.0).contains(&last.fraction)
            {
                return Err(PhysicsError::invalid(
                    "mass.fill_curve",
                    "invalid fill stations",
                ));
            }
            if fill >= first.fraction && fill <= last.fraction {
                let lower = vector(&first.cg, values)?;
                let upper = vector(&last.cg, values)?;
                let fraction = (fill - first.fraction) / (last.fraction - first.fraction);
                let mut cg = lower;
                for (value, upper) in cg.iter_mut().zip(upper) {
                    *value += fraction * (upper - *value);
                }
                return Ok(cg);
            }
        }
    }
    Err(PhysicsError::invalid(
        "mass.fill_curve",
        "fill fraction is outside the supplied curve",
    ))
}

#[cfg(test)]
mod tests;
