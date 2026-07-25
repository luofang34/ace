#![allow(clippy::expect_used, clippy::panic)]

use std::ffi::OsString;

use clap::Parser;

use crate::cli::Cli;

use super::{clap_detail, json_requested};

#[test]
fn detects_both_json_format_spellings() {
    assert!(json_requested(&[
        OsString::from("aex"),
        OsString::from("--format"),
        OsString::from("json"),
    ]));
    assert!(json_requested(&[
        OsString::from("aex"),
        OsString::from("--format=json"),
    ]));
    assert!(!json_requested(&[
        OsString::from("aex"),
        OsString::from("--format"),
        OsString::from("yaml"),
    ]));
}

#[test]
fn clap_failures_have_stable_codes_and_shape() {
    let error = Cli::try_parse_from(["aex", "analyze", "point", "--format", "json"])
        .expect_err("point arguments must be required");
    let detail = clap_detail(&error);

    assert_eq!(detail.code, "CLI_MISSING_ARGUMENT");
    assert_eq!(detail.path.as_deref(), Some("arguments"));
    assert_eq!(detail.context["kind"], "MissingRequiredArgument");
}
