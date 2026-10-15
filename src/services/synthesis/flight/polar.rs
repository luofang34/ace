use super::parameters::{FlightModel, positive};
use crate::{backends::openvsp::polar::MACH, domain::aerodynamics::PolarTable};
use ace_engine::SolverError;
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;

/// Below the lowest sampled Mach, compressibility changes k by well under the fit
/// tolerance, while linear extrapolation of the 0.2-0.5 slope can drive k toward
/// zero. Holding the lowest sample keeps low-speed phases usable without inventing
/// a trend; the transonic end remains unextrapolated.
const LOW_MACH_BASIS: &str = "k, CD0 and CL limit held at the lowest sampled Mach below it; no extrapolation above the highest sample";

pub(super) fn model(inputs: &BTreeMap<String, Quantity>) -> Result<FlightModel, SolverError> {
    let mut base = inputs.clone();
    // The table supplies k directly; this value only satisfies the shared native constructor.
    base.insert("oswald".into(), Quantity::new(1.0, Unit::Ratio)?);
    let mut model = FlightModel::new(&base)?;
    model.aero.cl_max = model
        .aero
        .cl_max
        .min(positive(inputs, "cl_limit", Unit::Ratio)?);
    let mut k = ["k_020", "k_050", "k_080"]
        .into_iter()
        .map(|key| positive(inputs, key, Unit::Ratio))
        .collect::<Result<Vec<_>, _>>()?;
    let lowest = k.first().copied().ok_or_else(|| {
        SolverError::Failed("VSPAERO polar supplies no sampled induced-drag factor".into())
    })?;
    k.insert(0, lowest);
    let mach = std::iter::once(0.0).chain(MACH).collect::<Vec<_>>();
    model.aero.polar_table = Some(PolarTable {
        cd0: vec![model.aero.cd0; mach.len()],
        cl_max: vec![model.aero.cl_max; mach.len()],
        mach,
        oswald_efficiency: None,
        induced_drag_factor: Some(k),
    });
    model.polar_basis = Some(LOW_MACH_BASIS);
    Ok(model)
}

#[cfg(test)]
mod tests;
