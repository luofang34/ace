//! Whole-vehicle geometry through the OpenVSP kernel: canonical shapes and placement frames
//! in, open interchange artifacts and intersected surface measurements out.

use crate::backends::{
    openvsp::{
        OpenVspBackend,
        kernel::{KernelObject, KernelShape},
    },
    registry::BackendRegistry,
};
use crate::domain::diagnostic::AexResult;
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec, Registry,
    SolverError,
};
use ace_model::{AnalysisResources, Artifact, Binding, ConceptShape, Quantity, Unit, VehicleModel};
use std::{collections::BTreeMap, sync::Arc};

mod placement;

const ID: &str = "vehicle.openvsp_geometry";
const METHOD: &str = "OpenVSP lofted elliptical bodies and NACA four-series trapezoidal wings in composed placement frames; untrimmed component surfaces in STEP and OBJ; CompGeom intersected wetted area and volume; no structure, airfoil calibration or certification claim";

pub(super) fn register_blocking(model: &VehicleModel, registry: &mut Registry) -> AexResult<()> {
    if !model
        .analyses
        .values()
        .any(|analysis| analysis.plugin == ID)
    {
        return Ok(());
    }
    let backends = BackendRegistry::detect_blocking();
    match backends
        .openvsp()
        .and_then(|backend| KernelPlugin::new_blocking(backend.clone()))
    {
        Ok(plugin) => registry.register(Arc::new(plugin))?,
        Err(error) => tracing::warn!(%error, "OpenVSP kernel geometry adapter unavailable"),
    }
    Ok(())
}

struct KernelPlugin {
    spec: PluginSpec,
    backend: OpenVspBackend,
    binary: String,
}

impl KernelPlugin {
    fn new_blocking(backend: OpenVspBackend) -> AexResult<Self> {
        let binary = backend.binary_identity_blocking()?;
        let identity = PluginIdentity {
            id: ID.into(),
            version: "1".into(),
            fidelity: Fidelity::Conceptual,
            code_revision: format!("{};{binary}", ace_engine::implementation_identity().digest),
        };
        Ok(Self {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain: Default::default(),
                identity,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([
                    ("wetted_area".into(), Unit::SquareMeter.into()),
                    ("volume".into(), Unit::CubicMeter.into()),
                ]),
            },
            backend,
            binary,
        })
    }

    fn evaluate_blocking(
        &self,
        resources: &AnalysisResources,
        values: &BTreeMap<String, Quantity>,
        control: &ace_engine::RunControl,
    ) -> Result<PluginOutput, SolverError> {
        let objects = resolve_objects(resources, values)?;
        if self.backend.binary_identity_blocking().map_err(external)? != self.binary {
            return Err(SolverError::Failed(
                "OpenVSP executable changed; reopen the model to bind its new identity".into(),
            ));
        }
        let products = self
            .backend
            .kernel_geometry_blocking(&objects, control)
            .map_err(external)?;
        let generator = self.spec.identity.generator();
        Ok(PluginOutput {
            values: BTreeMap::from([
                (
                    "wetted_area".into(),
                    Quantity::new(products.wetted_area_m2, Unit::SquareMeter)?,
                ),
                (
                    "volume".into(),
                    Quantity::new(products.volume_m3, Unit::CubicMeter)?,
                ),
            ]),
            approximate: true,
            artifacts: vec![
                Artifact::from_text(
                    products.vsp3,
                    "application/xml",
                    "openvsp.vsp3/1",
                    generator.clone(),
                ),
                Artifact::from_text(
                    products.step,
                    "model/step",
                    "step.ap203/1",
                    generator.clone(),
                ),
                Artifact::from_text(products.obj, "model/obj", "wavefront.obj/1", generator),
            ],
            failure: None,
            details: serde_json::json!({
                "backend": self.binary,
                "method": METHOD,
                "objects": objects,
                "components": products.components,
                "readback": products.readback,
            }),
        })
    }
}

impl AnalysisPlugin for KernelPlugin {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        _inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        self.evaluate_blocking(
            &context.resources,
            context.values,
            context.registry.control(),
        )
    }
}

/// Every selected object with a native shape, in its composed vehicle-frame placement.
fn resolve_objects(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> Result<Vec<KernelObject>, SolverError> {
    let mut objects = Vec::new();
    for (id, object) in &resources.geometry {
        let Some(shape) = &object.shape else { continue };
        let frame = ace_physics::resolve_frame(id, resources, values)
            .map_err(|error| SolverError::Failed(format!("{id}: {error}")))?;
        let shape = resolve_shape(id, shape, values)?;
        if matches!(
            shape,
            KernelShape::Wing {
                symmetric: true,
                ..
            }
        ) {
            placement::check_symmetric(id, &frame)?;
        }
        objects.push(KernelObject {
            id: id.clone(),
            shape,
            location_m: frame.translation,
            rotation_deg: placement::rotation_deg(frame.rotation),
        });
    }
    if objects.is_empty() {
        return Err(SolverError::Failed(
            "no selected geometry declares a native shape for the kernel".into(),
        ));
    }
    Ok(objects)
}

fn resolve_shape(
    id: &str,
    shape: &ConceptShape,
    values: &BTreeMap<String, Quantity>,
) -> Result<KernelShape, SolverError> {
    let scalar = |binding: &Binding, unit: Unit| {
        binding
            .resolve(values, unit)
            .map(|quantity| quantity.value)
            .map_err(|error| SolverError::Failed(format!("{id}: {error}")))
    };
    match shape {
        ConceptShape::Body {
            length,
            width,
            height,
            nose,
            tail,
        } => {
            let [length_m, width_m, height_m, nose_m, tail_m] = [
                scalar(length, Unit::Meter)?,
                scalar(width, Unit::Meter)?,
                scalar(height, Unit::Meter)?,
                scalar(nose, Unit::Meter)?,
                scalar(tail, Unit::Meter)?,
            ];
            if [length_m, width_m, height_m, nose_m, tail_m]
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
                || nose_m + tail_m >= length_m
            {
                return Err(SolverError::Failed(format!(
                    "{id}: body needs positive dimensions with nose + tail < length"
                )));
            }
            Ok(KernelShape::Body {
                length_m,
                width_m,
                height_m,
                nose_m,
                tail_m,
            })
        }
        ConceptShape::Wing {
            span,
            area,
            taper,
            sweep,
            dihedral,
            thickness,
            symmetric,
        } => resolve_wing(
            id,
            [
                scalar(span, Unit::Meter)?,
                scalar(area, Unit::SquareMeter)?,
                scalar(taper, Unit::Ratio)?,
                scalar(sweep, Unit::Degree)?,
                scalar(dihedral, Unit::Degree)?,
                scalar(thickness, Unit::Ratio)?,
            ],
            *symmetric,
        ),
    }
}

/// The same executable domain as the native concept surfaces, in degrees.
fn resolve_wing(
    id: &str,
    [span_m, area_m2, taper, sweep_deg, dihedral_deg, thickness]: [f64; 6],
    symmetric: bool,
) -> Result<KernelShape, SolverError> {
    if !(span_m.is_finite() && area_m2.is_finite() && span_m > 0.0 && area_m2 > 0.0)
        || !(0.05..=1.0).contains(&taper)
        || !(-70.0..=70.0).contains(&sweep_deg)
        || !(-30.0..=30.0).contains(&dihedral_deg)
        || !(0.01..=0.3).contains(&thickness)
    {
        return Err(SolverError::Failed(format!(
            "{id}: wing outside the span/area/taper/sweep/dihedral/thickness domain"
        )));
    }
    Ok(KernelShape::Wing {
        span_m,
        area_m2,
        taper,
        sweep_deg,
        dihedral_deg,
        thickness,
        symmetric,
    })
}

fn external(source: crate::domain::diagnostic::AexError) -> SolverError {
    SolverError::External {
        solver: ID.into(),
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;
