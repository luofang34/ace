use super::{FlowOperator, Gradient, Primitive, components, curved, flatten, limit};
use crate::operator::Workspace;

pub(in crate::operator) fn limit_shock_gradients(op: &FlowOperator, workspace: &mut Workspace) {
    for (i, gradient) in workspace.gradient.iter_mut().enumerate() {
        let sensor = workspace.shock_sensor[i];
        if sensor == 0.0 {
            continue;
        }
        let projected = flatten(*gradient, sensor);
        // Removing a shock-normal component rotates a slope. Its face values
        // can leave the bounds satisfied before projection, even as its norm falls.
        gradient.limited = if op.mesh.cells[i].radial_moments.is_some() {
            curved::limit_projected(op, &workspace.primitive, i, projected)
        } else {
            let limited = limit_planar(op, &workspace.primitive, i, projected);
            // Planar sources integrate the same linear pressure as the face traces.
            let [x, y, _] = limited[4];
            workspace.pressure[i] = [x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
            limited
        };
    }
}

fn limit_planar(
    op: &FlowOperator,
    states: &[Primitive],
    i: usize,
    gradient: Gradient,
) -> [[f64; 3]; 5] {
    let cell = &op.mesh.cells[i];
    let own = components(op.gas, states[i]);
    let (mut lower, mut upper) = (own, own);
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        if op.extrapolated_outflow(face) {
            continue;
        }
        let (q, _) = op.sample(face, i, states);
        let values = components(op.gas, q);
        for k in 0..5 {
            lower[k] = lower[k].min(values[k]);
            upper[k] = upper[k].max(values[k]);
        }
    }
    limit(op, cell, own, lower, upper, gradient.limited)
}
