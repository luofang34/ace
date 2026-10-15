use crate::{ModelError, ModelResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Identity of the tool that created an artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GeneratorIdentity {
    /// Generator or solver identifier.
    pub id: String,
    /// Generator contract version.
    pub version: String,
    /// Implementation and dependency identity supplied by the generator.
    pub implementation: String,
}

/// Content-addressed geometry, mesh, load case or solver result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// SHA-256 of the exact content bytes, as lowercase hexadecimal.
    pub digest: String,
    /// Declared media type.
    pub media_type: String,
    /// Content schema identifier, including its contract version.
    pub schema: String,
    /// Tool identity retained independently of the vehicle revision.
    pub generator: GeneratorIdentity,
    /// Optional local path or external locator; never an authority for content identity.
    pub location: Option<String>,
    /// Embedded UTF-8 content for replay without an external file.
    pub text: Option<String>,
}

impl Artifact {
    /// Capture a portable UTF-8 artifact using its exact encoded bytes.
    pub fn from_text(
        text: String,
        media_type: &str,
        schema: &str,
        generator: GeneratorIdentity,
    ) -> Self {
        Self {
            digest: hex::encode(Sha256::digest(text.as_bytes())),
            media_type: media_type.into(),
            schema: schema.into(),
            generator,
            location: None,
            text: Some(text),
        }
    }

    /// Verify an external file or embedded payload before interpreting it.
    pub fn verify(&self, bytes: &[u8]) -> ModelResult<()> {
        let actual = hex::encode(Sha256::digest(bytes));
        if actual != self.digest {
            return Err(ModelError::invalid(
                self.location.as_deref().unwrap_or(&self.schema),
                format!(
                    "artifact content changed: expected {}, got {actual}",
                    self.digest
                ),
            ));
        }
        Ok(())
    }

    /// Validate metadata and any embedded payload; external bytes require `verify` on access.
    pub fn validate(&self) -> ModelResult<()> {
        if self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.media_type.is_empty()
            || self.schema.is_empty()
            || self.generator.id.is_empty()
            || self.generator.version.is_empty()
            || self.generator.implementation.is_empty()
            || (self.text.is_none() && self.location.as_ref().is_none_or(|value| value.is_empty()))
        {
            return Err(ModelError::invalid(
                "artifact",
                "complete content address, schema, generator and content/locator are required",
            ));
        }
        if let Some(text) = &self.text {
            self.verify(text.as_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
