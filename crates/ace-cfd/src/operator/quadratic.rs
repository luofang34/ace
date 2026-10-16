use super::axisymmetry;
use crate::CfdError;
use crate::mesh::Mesh;

mod stencil;
pub(super) use stencil::{Stencil, stencils};

pub(super) fn coefficients(stencil: &Stencil, value: impl Fn(usize) -> f64, i: usize) -> [f64; 5] {
    let mut result = [0.0; 5];
    for (n, weight) in &stencil.neighbors {
        let difference = value(*n) - value(i);
        for (c, w) in result.iter_mut().zip(weight) {
            *c += w * difference;
        }
    }
    result
}
