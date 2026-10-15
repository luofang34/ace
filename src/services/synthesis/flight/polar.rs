use super::parameters::{FlightModel, positive};
use crate::{
    backends::openvsp::polar::{MACH, PORTS},
    domain::aerodynamics::PolarTable,
};
use ace_engine::SolverError;
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn model(inputs: &BTreeMap<String, Quantity>) -> Result<FlightModel, SolverError> {
    let mut base = inputs.clone();
    // The table supplies k directly; this value only satisfies the shared native constructor.
    base.insert("oswald".into(), Quantity::new(1.0, Unit::Ratio)?);
    let mut model = FlightModel::new(&base)?;
    model.aero.cl_max = model
        .aero
        .cl_max
        .min(positive(inputs, "cl_limit", Unit::Ratio)?);
    let k = PORTS
        .into_iter()
        .take(MACH.len())
        .map(|key| positive(inputs, key, Unit::Ratio))
        .collect::<Result<Vec<_>, _>>()?;
    model.aero.polar_table = Some(PolarTable {
        mach: MACH.to_vec(),
        cd0: vec![model.aero.cd0; MACH.len()],
        cl_max: vec![model.aero.cl_max; MACH.len()],
        oswald_efficiency: None,
        induced_drag_factor: Some(k),
    });
    Ok(model)
}

#[cfg(test)]
mod tests;
