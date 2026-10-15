use crate::{PhysicsError, PhysicsResult, bindings::vector};
use ace_model::{AnalysisResources, Quantity};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Transform {
    pub rotation: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

impl Transform {
    pub fn point(&self, point: [f64; 3]) -> PhysicsResult<[f64; 3]> {
        let result = std::array::from_fn(|i| {
            self.translation[i]
                + self.rotation[i]
                    .iter()
                    .zip(point)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
        });
        if result.iter().any(|value| !value.is_finite()) {
            return Err(PhysicsError::invalid(
                "frame",
                "nonfinite transformed point",
            ));
        }
        Ok(result)
    }

    pub fn tensor(&self, tensor: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let left = multiply(self.rotation, tensor);
        let transposed = std::array::from_fn(|i| std::array::from_fn(|j| self.rotation[j][i]));
        multiply(left, transposed)
    }
}

pub(crate) fn resolve(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Transform> {
    let mut transform = Transform {
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };
    let mut current = Some(id);
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return Err(PhysicsError::invalid(id, "coordinate frame cycle"));
        }
        let object = resources
            .geometry
            .get(id)
            .or_else(|| resources.frames.get(id))
            .ok_or_else(|| PhysicsError::invalid(id, "coordinate frame is unavailable"))?;
        object.placement.validate(id)?;
        let parent = Transform {
            rotation: object.placement.rotation,
            translation: vector(&object.placement.translation, values)?,
        };
        transform.translation = parent.point(transform.translation)?;
        transform.rotation = multiply(parent.rotation, transform.rotation);
        current = object.placement.relative_to.as_deref();
    }
    Ok(transform)
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

#[cfg(test)]
mod tests;

pub(super) fn bounds(
    transform: &Transform,
    minimum: [f64; 3],
    maximum: [f64; 3],
) -> PhysicsResult<([f64; 3], [f64; 3])> {
    let mut lower = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    for corner in 0..8 {
        let point = std::array::from_fn(|axis| {
            if corner & (1 << axis) == 0 {
                minimum[axis]
            } else {
                maximum[axis]
            }
        });
        let point = transform.point(point)?;
        for axis in 0..3 {
            lower[axis] = lower[axis].min(point[axis]);
            upper[axis] = upper[axis].max(point[axis]);
        }
    }
    Ok((lower, upper))
}
