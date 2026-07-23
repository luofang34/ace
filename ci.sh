#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --no-deps
cargo build --release
