use crate::{DimensionalValue, ModelError, ModelResult, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Serializable dimensional arithmetic tree; no executable source or dynamic imports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    /// Explicit dimensional constant.
    Constant {
        /// Scalar literal.
        value: Quantity,
    },
    /// Reference to the single authoritative parameter map.
    Reference {
        /// Canonical parameter path.
        parameter: String,
    },
    /// Addition of compatible dimensions.
    Add {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Subtraction of compatible dimensions.
    Subtract {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Multiplication.
    Multiply {
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Division; zero denominators fail.
    Divide {
        /// Numerator.
        left: Box<Expression>,
        /// Denominator.
        right: Box<Expression>,
    },
    /// Bounded integer power.
    Power {
        /// Base expression.
        base: Box<Expression>,
        /// Integer exponent.
        exponent: i8,
    },
}

impl Expression {
    /// Gather referenced parameters while enforcing the expression complexity limit.
    pub fn dependencies(&self) -> ModelResult<BTreeSet<String>> {
        let mut dependencies = BTreeSet::new();
        let mut stack = vec![(self, 0_u32)];
        while let Some((node, depth)) = stack.pop() {
            if depth > 64 {
                return Err(ModelError::invalid(
                    "expression",
                    "nesting exceeds 64 levels",
                ));
            }
            match node {
                Self::Reference { parameter } => {
                    dependencies.insert(parameter.clone());
                }
                Self::Constant { value } => {
                    value.dimensional()?;
                }
                Self::Power { base, exponent } => {
                    if !(-16..=16).contains(exponent) {
                        return Err(ModelError::invalid(
                            "expression",
                            "power is outside -16..16",
                        ));
                    }
                    stack.push((base, depth.wrapping_add(1)));
                }
                Self::Add { left, right }
                | Self::Subtract { left, right }
                | Self::Multiply { left, right }
                | Self::Divide { left, right } => {
                    stack.extend([
                        (left.as_ref(), depth.wrapping_add(1)),
                        (right.as_ref(), depth.wrapping_add(1)),
                    ]);
                }
            }
        }
        Ok(dependencies)
    }

    /// Evaluate declared inputs and normalize to the output port's unit.
    pub fn evaluate(
        &self,
        values: &BTreeMap<String, Quantity>,
        unit: Unit,
    ) -> ModelResult<Quantity> {
        self.dependencies()?;
        self.evaluate_inner(values)?.to(unit)
    }

    fn evaluate_inner(&self, values: &BTreeMap<String, Quantity>) -> ModelResult<DimensionalValue> {
        match self {
            Self::Constant { value } => value.dimensional(),
            Self::Reference { parameter } => values
                .get(parameter)
                .ok_or_else(|| ModelError::UnknownReference(parameter.clone()))?
                .dimensional(),
            Self::Add { left, right } => left
                .evaluate_inner(values)?
                .add(right.evaluate_inner(values)?, false),
            Self::Subtract { left, right } => left
                .evaluate_inner(values)?
                .add(right.evaluate_inner(values)?, true),
            Self::Multiply { left, right } => left
                .evaluate_inner(values)?
                .multiply(right.evaluate_inner(values)?, false),
            Self::Divide { left, right } => left
                .evaluate_inner(values)?
                .multiply(right.evaluate_inner(values)?, true),
            Self::Power { base, exponent } => base.evaluate_inner(values)?.power(*exponent),
        }
    }
}
