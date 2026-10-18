use super::{AnalysisContext, AnalysisPlugin, Fidelity, PluginOutput, PluginSpec};
use crate::{EngineError, EngineResult, SolverError};
use ace_model::Quantity;
use std::collections::BTreeMap;
use std::sync::Arc;

type Key = (String, String);
type Plugins = BTreeMap<Key, Arc<dyn AnalysisPlugin>>;

/// Application-owned registry; model files cannot load arbitrary executable code.
#[derive(Clone, Default)]
pub struct Registry {
    plugins: Plugins,
    previews: Plugins,
    control: crate::RunControl,
}

impl Registry {
    /// Select fast adapters and explicit replacements for isolated previews.
    /// Replacement evidence retains the fast adapter's identity and is approximate.
    pub fn preview(&self) -> Self {
        let mut plugins: Plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| fast(plugin.spec()))
            .map(|(key, plugin)| (key.clone(), plugin.clone()))
            .collect();
        plugins.extend(self.previews.clone());
        Self {
            plugins,
            previews: self.previews.clone(),
            control: self.control.clone(),
        }
    }

    /// Execution control for native checkpoints and cancellable external adapters.
    pub fn control(&self) -> &crate::RunControl {
        &self.control
    }

    pub(crate) fn controlled(&self, control: crate::RunControl) -> Self {
        Self {
            plugins: self.plugins.clone(),
            previews: self.previews.clone(),
            control,
        }
    }

    /// Add a solver with a previously unused identifier/version.
    pub fn register(&mut self, plugin: Arc<dyn AnalysisPlugin>) -> EngineResult<()> {
        let identity = &plugin.spec().identity;
        plugin
            .spec()
            .validate()
            .map_err(|source| crate::EngineError::Contract {
                analysis: identity.id.clone(),
                source,
            })?;
        let key = (identity.id.clone(), identity.version.clone());
        if self.plugins.contains_key(&key) {
            return Err(EngineError::DuplicatePlugin(key.0, key.1));
        }
        self.plugins.insert(key, plugin);
        Ok(())
    }

    /// Bind a registered solver to a fast adapter with identical port names and units.
    /// This never substitutes the adapter during authoritative execution.
    pub fn register_preview(
        &mut self,
        id: &str,
        version: &str,
        plugin: Arc<dyn AnalysisPlugin>,
    ) -> EngineResult<()> {
        let key = (id.to_owned(), version.to_owned());
        let invalid = |reason: &str| EngineError::InvalidPreview {
            plugin: id.into(),
            version: version.into(),
            reason: reason.into(),
        };
        let original = self
            .plugins
            .get(&key)
            .ok_or_else(|| invalid("authoritative solver is not registered"))?;
        if self.previews.contains_key(&key) {
            return Err(invalid("preview adapter is already registered"));
        }
        let spec = plugin.spec();
        spec.validate()
            .map_err(|source| crate::EngineError::Contract {
                analysis: spec.identity.id.clone(),
                source,
            })?;
        if !fast(spec) {
            return Err(invalid("preview adapter must be conceptual or surrogate"));
        }
        if spec.mode != original.spec().mode
            || spec.inputs != original.spec().inputs
            || spec.outputs != original.spec().outputs
        {
            return Err(invalid(
                "preview ports and units must match the authoritative solver",
            ));
        }
        self.previews.insert(key, Arc::new(Preview { plugin }));
        Ok(())
    }

    pub(crate) fn get(&self, id: &str, version: &str) -> Option<&dyn AnalysisPlugin> {
        self.plugins
            .get(&(id.to_owned(), version.to_owned()))
            .map(Arc::as_ref)
    }
}

fn fast(spec: &PluginSpec) -> bool {
    matches!(
        spec.identity.fidelity,
        Fidelity::Conceptual | Fidelity::Surrogate
    )
}

struct Preview {
    plugin: Arc<dyn AnalysisPlugin>,
}

impl AnalysisPlugin for Preview {
    fn spec(&self) -> &PluginSpec {
        self.plugin.spec()
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let mut output = self.plugin.evaluate(inputs, context)?;
        output.approximate = true;
        Ok(output)
    }

    fn evaluate_phase(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        state: &ace_model::VehicleState,
        context: &AnalysisContext<'_>,
    ) -> Result<super::PhaseOutput, SolverError> {
        let mut output = self.plugin.evaluate_phase(inputs, state, context)?;
        output.approximate = true;
        Ok(output)
    }
}
