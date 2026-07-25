use std::io;
use std::path::PathBuf;

use serde_json::json;

use super::AexError;

#[test]
fn domain_codes_and_paths_are_preserved() {
    let detail = AexError::validation(
        "INCONSISTENT_WING_PLANFORM",
        "aircraft.geometry.wing",
        "area, span, and aspect ratio do not close",
    )
    .detail();

    assert_eq!(detail.code, "INCONSISTENT_WING_PLANFORM");
    assert_eq!(detail.path.as_deref(), Some("aircraft.geometry.wing"));
    assert_eq!(detail.message, "area, span, and aspect ratio do not close");
    assert_eq!(detail.context, json!({}));
}

#[test]
fn adapter_failures_have_stable_context() {
    let detail = AexError::Read {
        path: PathBuf::from("missing.yaml"),
        source: io::Error::from(io::ErrorKind::NotFound),
    }
    .detail();

    assert_eq!(detail.code, "FILE_READ_FAILED");
    assert_eq!(detail.path.as_deref(), Some("missing.yaml"));
    assert_eq!(detail.context["io_kind"], "NotFound");
}

#[test]
fn stored_record_includes_its_typed_cause() {
    let detail = AexError::StoredRecord {
        path: PathBuf::from("runs/corrupt.json"),
        source: Box::new(AexError::analysis("INVALID_CONTENT_ID", "digest mismatch")),
    }
    .detail();

    assert_eq!(detail.code, "INVALID_STORED_RECORD");
    assert_eq!(detail.context["cause"]["code"], "INVALID_CONTENT_ID");
    assert_eq!(detail.context["cause"]["path"], "analysis");
}

#[test]
fn backend_failures_identify_the_backend_path() {
    let detail = AexError::BackendUnavailable {
        backend: "openvsp".to_owned(),
        reason: "executable was not discovered".to_owned(),
    }
    .detail();

    assert_eq!(detail.code, "BACKEND_UNAVAILABLE");
    assert_eq!(detail.path.as_deref(), Some("backends.openvsp"));
    assert_eq!(detail.context["backend"], "openvsp");
}
