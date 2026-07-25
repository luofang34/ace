use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) fn digest_serializable<T: Serialize + ?Sized>(value: &T) -> AexResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|source| AexError::Json { source })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(crate) fn content_id<T: Serialize + ?Sized>(prefix: &str, value: &T) -> AexResult<String> {
    Ok(format!("{prefix}{}", digest_serializable(value)?))
}

pub(crate) fn validate_digest(digest: &str, path: &str) -> AexResult<()> {
    let valid = digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(AexError::validation(
            "INVALID_CONTENT_DIGEST",
            path,
            "expected a 64-character lowercase SHA-256 digest",
        ))
    }
}

pub(crate) fn validate_content_id(id: &str, prefix: &str, path: &str) -> AexResult<()> {
    let digest = id.strip_prefix(prefix).ok_or_else(|| {
        AexError::validation(
            "INVALID_CONTENT_ID",
            path,
            format!("expected identifier prefix {prefix}"),
        )
    })?;
    validate_digest(digest, path)
}

pub(crate) fn validate_safe_id(id: &str, path: &str) -> AexResult<()> {
    let valid = !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(AexError::validation(
            "INVALID_STABLE_ID",
            path,
            "use 1-80 ASCII letters, digits, hyphens, or underscores",
        ))
    }
}
