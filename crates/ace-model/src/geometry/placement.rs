use crate::{Binding, ModelError, ModelResult, Quantity, Unit};
use serde::{Deserialize, Serialize};

/// Rigid local-to-parent transform; each geometry identifier also names its local frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Parent coordinate frame; absence denotes vehicle coordinates.
    pub relative_to: Option<String>,
    /// Origin in the parent frame, metres.
    pub translation: [Binding; 3],
    /// Proper orthonormal rotation matrix mapping local vectors to parent axes.
    pub rotation: [[f64; 3]; 3],
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            relative_to: None,
            translation: std::array::from_fn(|_| {
                Binding::Constant(Quantity {
                    value: 0.0,
                    unit: Unit::Meter,
                })
            }),
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

impl Placement {
    /// Reject scaling, reflection and nonfinite transforms before executing physical models.
    pub fn validate(&self, id: &str) -> ModelResult<()> {
        let r = self.rotation;
        if r.iter().flatten().any(|value| !value.is_finite()) {
            return Err(ModelError::invalid(id, "rotation must be finite"));
        }
        for (i, row) in r.iter().enumerate() {
            for (j, other) in r.iter().enumerate() {
                let dot: f64 = row.iter().zip(other).map(|(a, b)| a * b).sum();
                if (dot - if i == j { 1.0 } else { 0.0 }).abs() > 1e-9 {
                    return Err(ModelError::invalid(id, "rotation must be orthonormal"));
                }
            }
        }
        let determinant = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
        if (determinant - 1.0).abs() > 1e-9 {
            return Err(ModelError::invalid(id, "rotation must preserve handedness"));
        }
        Ok(())
    }
}
