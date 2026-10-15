use crate::{PhysicsError, PhysicsResult};
use ace_model::{Binding, ModelError, Quantity, Unit};
use std::collections::BTreeMap;

pub(crate) fn scalar(
    binding: &Binding,
    values: &BTreeMap<String, Quantity>,
    unit: Unit,
) -> PhysicsResult<f64> {
    Ok(binding.resolve(values, unit)?.value)
}

pub(crate) fn vector(
    bindings: &[Binding; 3],
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<[f64; 3]> {
    let mut result = [0.0; 3];
    for (value, binding) in result.iter_mut().zip(bindings) {
        *value = scalar(binding, values, Unit::Meter)?;
    }
    Ok(result)
}

pub(crate) fn quantity(
    binding: &Binding,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Quantity> {
    match binding {
        Binding::Constant(value) => Ok(Quantity::new(value.value, value.unit)?),
        Binding::Parameter(name) => values
            .get(name)
            .copied()
            .ok_or_else(|| ModelError::UnknownReference(name.clone()).into()),
    }
}

pub(crate) fn nonnegative(object: &str, value: f64) -> PhysicsResult<f64> {
    if !value.is_finite() || value < 0.0 {
        return Err(PhysicsError::invalid(
            object,
            "value must be finite and nonnegative",
        ));
    }
    Ok(value)
}
